mod ocr;
mod vision;

use std::collections::{HashSet, VecDeque};
use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

pub use ocr::OcrBackend;
pub use ocr::OcrError;
pub use ocr::OcrLine;
pub use ocr::OcrRequest;
pub use ocr::OcrResult;
pub use ocr::recognize_text_from_png;
pub use vision::InventoryPageSignature;
pub use vision::InventoryPageSignatureComparison;
pub use vision::InventoryPageSignatureEntry;
pub use vision::InventoryPageSignatureError;
pub use vision::LowConfidencePolicy;
pub use vision::MarkerMatchResult;
pub use vision::PageActionDefinition;
pub use vision::PageConfirmationError;
pub use vision::PageConfirmationMarker;
pub use vision::PageConfirmationResult;
pub use vision::PageConfirmationStrategy;
pub use vision::PageStateCatalog;
pub use vision::PageStateDefinition;
pub use vision::ReferenceResolution;
pub use vision::ResolvedRoiRect;
pub use vision::RoiArtifactPayload;
pub use vision::RoiCropError;
pub use vision::RoiCropResult;
pub use vision::RoiDefinition;
pub use vision::RoiPreprocessStep;
pub use vision::RoiPurpose;
pub use vision::RoiRect;
pub use vision::VisionConfigError;
pub use vision::build_inventory_page_signature;
pub use vision::compare_inventory_page_signatures;
pub use vision::crop_all_rois_from_png;
pub use vision::crop_single_roi_from_png;
pub use vision::evaluate_page_confirmation_from_png;
pub use vision::load_page_state_catalog_from_json;
pub use vision::load_page_state_catalog_from_path;

pub const DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT: u16 = 8;
const DEFAULT_MUMU_LOOPBACK_PORT: u16 = 7555;
const DEFAULT_MUMU_PORT_SERIES_START: u16 = 16_384;
const DEFAULT_MUMU_PORT_SERIES_STEP: u16 = 32;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceConnectRequest {
    pub adb_executable: Option<PathBuf>,
    pub preferred_serial: Option<String>,
    pub discovery_instance_count: u16,
}

impl Default for DeviceConnectRequest {
    fn default() -> Self {
        Self {
            adb_executable: None,
            preferred_serial: None,
            discovery_instance_count: DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotCaptureRequest {
    pub adb_executable: Option<PathBuf>,
    pub preferred_serial: Option<String>,
    pub discovery_instance_count: u16,
}

impl Default for ScreenshotCaptureRequest {
    fn default() -> Self {
        Self {
            adb_executable: None,
            preferred_serial: None,
            discovery_instance_count: DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT,
        }
    }
}

impl From<&ScreenshotCaptureRequest> for DeviceConnectRequest {
    fn from(value: &ScreenshotCaptureRequest) -> Self {
        Self {
            adb_executable: value.adb_executable.clone(),
            preferred_serial: value.preferred_serial.clone(),
            discovery_instance_count: value.discovery_instance_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDeviceEntry {
    pub serial: String,
    pub state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSelectionSource {
    PreferredSerial,
    ExistingConnectedDevice,
    ConnectedCandidate,
}

impl DeviceSelectionSource {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::PreferredSerial => "手动指定",
            Self::ExistingConnectedDevice => "已连接设备",
            Self::ConnectedCandidate => "自动发现并连接",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceConnectionInfo {
    pub adb_executable: PathBuf,
    pub serial: String,
    pub selection_source: DeviceSelectionSource,
    pub visible_devices: Vec<AdbDeviceEntry>,
    pub attempted_serials: Vec<String>,
}

pub struct DeviceSession {
    adb_executable: PathBuf,
    serial: String,
    selection_source: DeviceSelectionSource,
}

impl DeviceSession {
    pub fn adb_executable(&self) -> &Path {
        &self.adb_executable
    }

    pub fn serial(&self) -> &str {
        self.serial.as_str()
    }

    pub fn selection_source(&self) -> DeviceSelectionSource {
        self.selection_source
    }

    pub fn capture_screenshot_png(&self) -> Result<Vec<u8>, ScreenshotCaptureError> {
        capture_screenshot_for_session_with_runner(self, &ProcessCommandRunner)
    }

    pub fn send_input(&self, action: &DeviceInputAction) -> Result<(), DeviceInputError> {
        send_input_for_session_with_runner(self, action, &ProcessCommandRunner)
    }
}

pub struct DeviceSessionOpenResult {
    pub session: DeviceSession,
    pub connection: DeviceConnectionInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotCaptureResult {
    pub png_bytes: Vec<u8>,
    pub connection: DeviceConnectionInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceInputAction {
    Tap {
        x: u32,
        y: u32,
    },
    Swipe {
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    },
    KeyEvent {
        key_code: u16,
    },
}

impl DeviceInputAction {
    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Tap { .. } => "点按",
            Self::Swipe { .. } => "滑动",
            Self::KeyEvent { .. } => "按键",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInputRequest {
    pub adb_executable: Option<PathBuf>,
    pub preferred_serial: Option<String>,
    pub discovery_instance_count: u16,
    pub action: DeviceInputAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInputResult {
    pub connection: DeviceConnectionInfo,
    pub action: DeviceInputAction,
}

pub fn open_device_session(
    request: &DeviceConnectRequest,
) -> Result<DeviceSessionOpenResult, DeviceConnectError> {
    open_device_session_with_runner(request, &ProcessCommandRunner)
}

pub fn capture_device_screenshot(
    request: &ScreenshotCaptureRequest,
) -> Result<ScreenshotCaptureResult, ScreenshotCaptureError> {
    capture_device_screenshot_with_runner(request, &ProcessCommandRunner)
}

pub fn capture_device_screenshot_png(
    request: &ScreenshotCaptureRequest,
) -> Result<Vec<u8>, ScreenshotCaptureError> {
    capture_device_screenshot(request).map(|result| result.png_bytes)
}

pub fn send_device_input(
    request: &DeviceInputRequest,
) -> Result<DeviceInputResult, DeviceInputError> {
    send_device_input_with_runner(request, &ProcessCommandRunner)
}

#[derive(Debug, Error)]
pub enum DeviceConnectError {
    #[error(
        "未找到可用的 adb 可执行文件；请在设置里填写 `adb.executable`，或确保 `adb.exe` 在 PATH / MuMu 安装目录中可发现"
    )]
    AdbExecutableNotFound,
    #[error("配置的 adb 可执行文件不存在：{path}")]
    ConfiguredAdbNotFound { path: PathBuf },
    #[error("执行 `{action}` 失败：{source}")]
    CommandIo {
        action: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("`{action}` 执行失败（退出码 {status_code}）：{details}")]
    CommandFailed {
        action: &'static str,
        status_code: i32,
        details: String,
    },
    #[error(
        "未找到可连接的 MuMu 设备；已尝试：{attempted_serials}；当前 adb devices：{visible_devices}"
    )]
    NoDeviceFound {
        attempted_serials: String,
        visible_devices: String,
    },
    #[error("手动指定的设备 `{serial}` 当前不可用；当前 adb devices：{visible_devices}")]
    PreferredSerialUnavailable {
        serial: String,
        visible_devices: String,
    },
}

#[derive(Debug, Error)]
pub enum ScreenshotCaptureError {
    #[error("{0}")]
    Connect(#[from] DeviceConnectError),
    #[error("执行 `adb exec-out screencap -p` 失败：{details}")]
    ScreenshotCommandFailed { details: String },
    #[error("设备返回的截图不是有效 PNG")]
    InvalidPng,
}

#[derive(Debug, Error)]
pub enum DeviceInputError {
    #[error("{0}")]
    Connect(#[from] DeviceConnectError),
    #[error("执行设备输入 `{action}` 失败：{details}")]
    InputCommandFailed {
        action: &'static str,
        details: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandOutput {
    status_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

trait CommandRunner {
    fn run(&self, executable: &Path, args: &[String]) -> Result<CommandOutput, io::Error>;
}

struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, executable: &Path, args: &[String]) -> Result<CommandOutput, io::Error> {
        let output = Command::new(executable).args(args).output()?;
        Ok(CommandOutput {
            status_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn open_device_session_with_runner<R: CommandRunner>(
    request: &DeviceConnectRequest,
    runner: &R,
) -> Result<DeviceSessionOpenResult, DeviceConnectError> {
    let adb_executable = resolve_adb_executable(request.adb_executable.as_deref())?;
    run_required_command(
        runner,
        &adb_executable,
        &["start-server"],
        "adb start-server",
    )?;

    let preferred_serial = normalize_serial_input(request.preferred_serial.as_deref());
    let mut visible_devices = list_adb_devices(runner, &adb_executable)?;
    let mut attempted_serials = Vec::new();

    if let Some(serial) = preferred_serial.as_ref() {
        if let Some(device) = find_device(&visible_devices, serial)
            && device.state == "device"
        {
            return Ok(build_open_result(
                adb_executable,
                serial.clone(),
                DeviceSelectionSource::PreferredSerial,
                visible_devices,
                attempted_serials,
            ));
        }

        if looks_like_endpoint(serial) {
            attempted_serials.push(serial.clone());
            if try_connect_endpoint(runner, &adb_executable, serial)? {
                visible_devices = list_adb_devices(runner, &adb_executable)?;
                if let Some(device) = find_device(&visible_devices, serial)
                    && device.state == "device"
                {
                    return Ok(build_open_result(
                        adb_executable,
                        serial.clone(),
                        DeviceSelectionSource::PreferredSerial,
                        visible_devices,
                        attempted_serials,
                    ));
                }
            }
        }

        return Err(DeviceConnectError::PreferredSerialUnavailable {
            serial: serial.clone(),
            visible_devices: render_visible_devices(&visible_devices),
        });
    }

    let candidates = build_mumu_serial_candidates(request.discovery_instance_count);
    for candidate in &candidates {
        if let Some(device) = find_device(&visible_devices, candidate)
            && device.state == "device"
        {
            return Ok(build_open_result(
                adb_executable,
                candidate.clone(),
                DeviceSelectionSource::ExistingConnectedDevice,
                visible_devices,
                attempted_serials,
            ));
        }
    }

    for candidate in &candidates {
        attempted_serials.push(candidate.clone());
        if !try_connect_endpoint(runner, &adb_executable, candidate)? {
            continue;
        }

        visible_devices = list_adb_devices(runner, &adb_executable)?;
        if let Some(device) = find_device(&visible_devices, candidate)
            && device.state == "device"
        {
            return Ok(build_open_result(
                adb_executable,
                candidate.clone(),
                DeviceSelectionSource::ConnectedCandidate,
                visible_devices,
                attempted_serials,
            ));
        }
    }

    Err(DeviceConnectError::NoDeviceFound {
        attempted_serials: attempted_serials.join(", "),
        visible_devices: render_visible_devices(&visible_devices),
    })
}

fn build_open_result(
    adb_executable: PathBuf,
    serial: String,
    selection_source: DeviceSelectionSource,
    visible_devices: Vec<AdbDeviceEntry>,
    attempted_serials: Vec<String>,
) -> DeviceSessionOpenResult {
    DeviceSessionOpenResult {
        session: DeviceSession {
            adb_executable: adb_executable.clone(),
            serial: serial.clone(),
            selection_source,
        },
        connection: DeviceConnectionInfo {
            adb_executable,
            serial,
            selection_source,
            visible_devices,
            attempted_serials,
        },
    }
}

fn capture_device_screenshot_with_runner<R: CommandRunner>(
    request: &ScreenshotCaptureRequest,
    runner: &R,
) -> Result<ScreenshotCaptureResult, ScreenshotCaptureError> {
    let opened = open_device_session_with_runner(&DeviceConnectRequest::from(request), runner)?;
    let png_bytes = capture_screenshot_for_session_with_runner(&opened.session, runner)?;

    Ok(ScreenshotCaptureResult {
        png_bytes,
        connection: opened.connection,
    })
}

fn send_device_input_with_runner<R: CommandRunner>(
    request: &DeviceInputRequest,
    runner: &R,
) -> Result<DeviceInputResult, DeviceInputError> {
    let opened = open_device_session_with_runner(
        &DeviceConnectRequest {
            adb_executable: request.adb_executable.clone(),
            preferred_serial: request.preferred_serial.clone(),
            discovery_instance_count: request.discovery_instance_count,
        },
        runner,
    )?;
    send_input_for_session_with_runner(&opened.session, &request.action, runner)?;

    Ok(DeviceInputResult {
        connection: opened.connection,
        action: request.action.clone(),
    })
}

fn capture_screenshot_for_session_with_runner<R: CommandRunner>(
    session: &DeviceSession,
    runner: &R,
) -> Result<Vec<u8>, ScreenshotCaptureError> {
    match capture_png_once(runner, session) {
        Ok(bytes) => Ok(bytes),
        Err(error) if looks_like_endpoint(session.serial()) => {
            tracing::warn!(
                serial = session.serial(),
                %error,
                "device screenshot capture failed; retrying after adb reconnect"
            );

            reconnect_endpoint(runner, session).map_err(|error| {
                ScreenshotCaptureError::ScreenshotCommandFailed {
                    details: error.to_string(),
                }
            })?;
            capture_png_once(runner, session)
        }
        Err(error) => Err(error),
    }
}

fn send_input_for_session_with_runner<R: CommandRunner>(
    session: &DeviceSession,
    action: &DeviceInputAction,
    runner: &R,
) -> Result<(), DeviceInputError> {
    match send_input_once(runner, session, action) {
        Ok(()) => Ok(()),
        Err(error) if looks_like_endpoint(session.serial()) => {
            tracing::warn!(
                serial = session.serial(),
                action = action.label_zh(),
                %error,
                "device input failed; retrying after adb reconnect"
            );
            reconnect_endpoint(runner, session).map_err(DeviceInputError::from)?;
            send_input_once(runner, session, action)
        }
        Err(error) => Err(error),
    }
}

fn capture_png_once<R: CommandRunner>(
    runner: &R,
    session: &DeviceSession,
) -> Result<Vec<u8>, ScreenshotCaptureError> {
    let args = vec![
        "-s".to_string(),
        session.serial().to_string(),
        "exec-out".to_string(),
        "screencap".to_string(),
        "-p".to_string(),
    ];
    let output = runner
        .run(session.adb_executable(), &args)
        .map_err(|source| ScreenshotCaptureError::ScreenshotCommandFailed {
            details: source.to_string(),
        })?;

    if output.status_code != 0 {
        return Err(ScreenshotCaptureError::ScreenshotCommandFailed {
            details: render_command_details(&output.stdout, &output.stderr, output.status_code),
        });
    }

    if !is_valid_png(&output.stdout) {
        return Err(ScreenshotCaptureError::InvalidPng);
    }

    Ok(output.stdout)
}

fn send_input_once<R: CommandRunner>(
    runner: &R,
    session: &DeviceSession,
    action: &DeviceInputAction,
) -> Result<(), DeviceInputError> {
    let mut args = vec![
        "-s".to_string(),
        session.serial().to_string(),
        "shell".to_string(),
        "input".to_string(),
    ];

    match action {
        DeviceInputAction::Tap { x, y } => {
            args.push("tap".to_string());
            args.push(x.to_string());
            args.push(y.to_string());
        }
        DeviceInputAction::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        } => {
            args.push("swipe".to_string());
            args.push(x1.to_string());
            args.push(y1.to_string());
            args.push(x2.to_string());
            args.push(y2.to_string());
            args.push(duration_ms.to_string());
        }
        DeviceInputAction::KeyEvent { key_code } => {
            args.push("keyevent".to_string());
            args.push(key_code.to_string());
        }
    }

    let output = runner
        .run(session.adb_executable(), &args)
        .map_err(|source| DeviceInputError::InputCommandFailed {
            action: action.label_zh(),
            details: source.to_string(),
        })?;

    if output.status_code != 0 {
        return Err(DeviceInputError::InputCommandFailed {
            action: action.label_zh(),
            details: render_command_details(&output.stdout, &output.stderr, output.status_code),
        });
    }

    Ok(())
}

fn reconnect_endpoint<R: CommandRunner>(
    runner: &R,
    session: &DeviceSession,
) -> Result<(), DeviceConnectError> {
    let disconnect_args = vec!["disconnect".to_string(), session.serial().to_string()];
    let _ = runner
        .run(session.adb_executable(), &disconnect_args)
        .map_err(|source| DeviceConnectError::CommandIo {
            action: "adb disconnect",
            source,
        })?;

    if !try_connect_endpoint(runner, session.adb_executable(), session.serial())? {
        return Err(DeviceConnectError::PreferredSerialUnavailable {
            serial: session.serial().to_string(),
            visible_devices: "重连后未重新出现在 adb devices 中".to_string(),
        });
    }

    let visible_devices = list_adb_devices(runner, session.adb_executable())?;
    let Some(device) = find_device(&visible_devices, session.serial()) else {
        return Err(DeviceConnectError::PreferredSerialUnavailable {
            serial: session.serial().to_string(),
            visible_devices: render_visible_devices(&visible_devices),
        });
    };

    if device.state != "device" {
        return Err(DeviceConnectError::PreferredSerialUnavailable {
            serial: session.serial().to_string(),
            visible_devices: render_visible_devices(&visible_devices),
        });
    }

    Ok(())
}

fn list_adb_devices<R: CommandRunner>(
    runner: &R,
    adb_executable: &Path,
) -> Result<Vec<AdbDeviceEntry>, DeviceConnectError> {
    let output = run_required_command(runner, adb_executable, &["devices"], "adb devices")?;
    Ok(parse_adb_devices_output(&output.stdout))
}

fn run_required_command<R: CommandRunner>(
    runner: &R,
    adb_executable: &Path,
    args: &[&str],
    action: &'static str,
) -> Result<CommandOutput, DeviceConnectError> {
    let string_args = args
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let output = runner
        .run(adb_executable, &string_args)
        .map_err(|source| DeviceConnectError::CommandIo { action, source })?;

    if output.status_code != 0 {
        return Err(DeviceConnectError::CommandFailed {
            action,
            status_code: output.status_code,
            details: render_command_details(&output.stdout, &output.stderr, output.status_code),
        });
    }

    Ok(output)
}

fn try_connect_endpoint<R: CommandRunner>(
    runner: &R,
    adb_executable: &Path,
    serial: &str,
) -> Result<bool, DeviceConnectError> {
    let args = vec!["connect".to_string(), serial.to_string()];
    let output =
        runner
            .run(adb_executable, &args)
            .map_err(|source| DeviceConnectError::CommandIo {
                action: "adb connect",
                source,
            })?;

    if output.status_code == 0 {
        tracing::info!(
            serial,
            stdout = %String::from_utf8_lossy(&output.stdout),
            "adb connect completed"
        );
        return Ok(true);
    }

    tracing::warn!(
        serial,
        status_code = output.status_code,
        details = %render_command_details(&output.stdout, &output.stderr, output.status_code),
        "adb connect candidate failed"
    );
    Ok(false)
}

fn parse_adb_devices_output(output: &[u8]) -> Vec<AdbDeviceEntry> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed == "List of devices attached"
                || trimmed.starts_with('*')
            {
                return None;
            }

            let mut parts = trimmed.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            Some(AdbDeviceEntry {
                serial: serial.to_string(),
                state: state.to_string(),
            })
        })
        .collect()
}

fn find_device<'a>(devices: &'a [AdbDeviceEntry], serial: &str) -> Option<&'a AdbDeviceEntry> {
    devices.iter().find(|device| device.serial == serial)
}

fn build_mumu_serial_candidates(discovery_instance_count: u16) -> Vec<String> {
    let max_instances = if discovery_instance_count == 0 {
        DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT
    } else {
        discovery_instance_count
    };

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    let first = loopback_serial(DEFAULT_MUMU_LOOPBACK_PORT);
    seen.insert(first.clone());
    candidates.push(first);

    for index in 0..usize::from(max_instances) {
        let port = DEFAULT_MUMU_PORT_SERIES_START + DEFAULT_MUMU_PORT_SERIES_STEP * index as u16;
        let serial = loopback_serial(port);
        if seen.insert(serial.clone()) {
            candidates.push(serial);
        }
    }

    candidates
}

fn normalize_serial_input(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(loopback_serial(trimmed.parse::<u16>().ok()?));
    }

    if let Some(port) = trimmed.strip_prefix("localhost:") {
        return Some(loopback_serial(port.parse::<u16>().ok()?));
    }

    Some(trimmed.to_string())
}

fn loopback_serial(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

fn looks_like_endpoint(serial: &str) -> bool {
    serial.starts_with("127.0.0.1:") || serial.starts_with("localhost:")
}

fn resolve_adb_executable(explicit: Option<&Path>) -> Result<PathBuf, DeviceConnectError> {
    if let Some(explicit) = explicit {
        return resolve_explicit_adb_executable(explicit);
    }

    if let Some(found) = search_path_for_command("adb") {
        return Ok(found);
    }

    search_common_adb_locations().ok_or(DeviceConnectError::AdbExecutableNotFound)
}

fn resolve_explicit_adb_executable(explicit: &Path) -> Result<PathBuf, DeviceConnectError> {
    if explicit.is_file() {
        return Ok(explicit.to_path_buf());
    }

    if explicit.components().count() > 1 {
        return Err(DeviceConnectError::ConfiguredAdbNotFound {
            path: explicit.to_path_buf(),
        });
    }

    search_path_for_command(explicit.as_os_str())
        .or_else(search_common_adb_locations)
        .ok_or_else(|| DeviceConnectError::ConfiguredAdbNotFound {
            path: explicit.to_path_buf(),
        })
}

fn search_path_for_command(command: impl AsRef<OsStr>) -> Option<PathBuf> {
    let command = command.as_ref();
    let command_path = Path::new(command);
    if command_path.is_file() {
        return Some(command_path.to_path_buf());
    }

    let has_extension = command_path.extension().is_some();
    let pathexts = executable_extensions();
    let path = env::var_os("PATH")?;

    for directory in env::split_paths(&path) {
        if has_extension {
            let candidate = directory.join(command_path);
            if candidate.is_file() {
                return Some(candidate);
            }
            continue;
        }

        for extension in &pathexts {
            let mut file_name = OsString::from(command);
            file_name.push(extension);
            let candidate = directory.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn executable_extensions() -> Vec<OsString> {
    let configured = env::var_os("PATHEXT")
        .map(|value| {
            env::split_paths(&value)
                .map(|path| path.into_os_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if configured.is_empty() {
        vec![
            OsString::from(".EXE"),
            OsString::from(".CMD"),
            OsString::from(".BAT"),
            OsString::from(".COM"),
        ]
    } else {
        configured
    }
}

fn search_common_adb_locations() -> Option<PathBuf> {
    let mut candidates = VecDeque::new();

    for env_key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        let Some(base) = env::var_os(env_key).map(PathBuf::from) else {
            continue;
        };

        for relative in [
            PathBuf::from("YXArkNights-12.0")
                .join("shell")
                .join("adb.exe"),
            PathBuf::from("MuMuPlayer-12.0")
                .join("shell")
                .join("adb.exe"),
            PathBuf::from("MuMuPlayerGlobal-12.0")
                .join("shell")
                .join("adb.exe"),
            PathBuf::from("Netease")
                .join("MuMuPlayer-12.0")
                .join("shell")
                .join("adb.exe"),
            PathBuf::from("Netease")
                .join("MuMuPlayerGlobal-12.0")
                .join("shell")
                .join("adb.exe"),
            PathBuf::from("Netease")
                .join("YXArkNights-12.0")
                .join("shell")
                .join("adb.exe"),
        ] {
            candidates.push_back(base.join(relative));
        }

        candidates.extend(scan_shell_adb_children(&base));
        candidates.extend(scan_shell_adb_children(&base.join("Netease")));
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn scan_shell_adb_children(base: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy().to_lowercase();
        if !["mumu", "arknight", "arknights", "yx"]
            .iter()
            .any(|marker| file_name.contains(marker))
        {
            continue;
        }

        let candidate = path.join("shell").join("adb.exe");
        if candidate.is_file() {
            results.push(candidate);
        }
    }

    results
}

fn render_visible_devices(devices: &[AdbDeviceEntry]) -> String {
    if devices.is_empty() {
        "<空>".to_string()
    } else {
        devices
            .iter()
            .map(|device| format!("{} ({})", device.serial, device.state))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn render_command_details(stdout: &[u8], stderr: &[u8], status_code: i32) -> String {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => format!("无输出（退出码 {status_code}）"),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stderr} | stdout: {stdout}"),
    }
}

fn is_valid_png(bytes: &[u8]) -> bool {
    bytes.len() >= PNG_SIGNATURE.len() && bytes[..PNG_SIGNATURE.len()] == PNG_SIGNATURE
}

#[cfg(test)]
mod tests {
    use super::AdbDeviceEntry;
    use super::CommandOutput;
    use super::DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT;
    use super::DeviceConnectRequest;
    use super::DeviceInputAction;
    use super::DeviceInputError;
    use super::DeviceInputRequest;
    use super::DeviceSelectionSource;
    use super::ScreenshotCaptureError;
    use super::ScreenshotCaptureRequest;
    use super::build_mumu_serial_candidates;
    use super::capture_device_screenshot_with_runner;
    use super::open_device_session_with_runner;
    use super::parse_adb_devices_output;
    use super::send_device_input_with_runner;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io;
    use std::path::{Path, PathBuf};

    const TEST_PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];

    #[test]
    fn request_can_store_optional_adb_path_and_preferred_serial() {
        let request = ScreenshotCaptureRequest {
            adb_executable: Some(PathBuf::from("C:/MuMu/shell/adb.exe")),
            preferred_serial: Some("127.0.0.1:7555".to_string()),
            discovery_instance_count: DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT,
        };

        assert_eq!(
            request.adb_executable,
            Some(PathBuf::from("C:/MuMu/shell/adb.exe"))
        );
        assert_eq!(request.preferred_serial.as_deref(), Some("127.0.0.1:7555"));
    }

    #[test]
    fn candidate_ports_start_with_default_mumu_endpoints() {
        let candidates = build_mumu_serial_candidates(3);
        assert_eq!(
            candidates,
            vec![
                "127.0.0.1:7555".to_string(),
                "127.0.0.1:16384".to_string(),
                "127.0.0.1:16416".to_string(),
                "127.0.0.1:16448".to_string(),
            ]
        );
    }

    #[test]
    fn adb_devices_parser_ignores_headers_and_daemon_lines() {
        let parsed = parse_adb_devices_output(
            b"* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\n127.0.0.1:7555\tdevice\n127.0.0.1:16384\toffline\n\n",
        );

        assert_eq!(
            parsed,
            vec![
                AdbDeviceEntry {
                    serial: "127.0.0.1:7555".to_string(),
                    state: "device".to_string(),
                },
                AdbDeviceEntry {
                    serial: "127.0.0.1:16384".to_string(),
                    state: "offline".to_string(),
                },
            ]
        );
    }

    #[test]
    fn capture_uses_existing_connected_device_without_connect() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:7555\tdevice\n".to_vec(),
            ),
            FakeCommandResult::success(TEST_PNG.to_vec()),
        ]);
        let request = ScreenshotCaptureRequest {
            adb_executable: Some(PathBuf::from("adb")),
            preferred_serial: None,
            discovery_instance_count: 4,
        };

        let result = capture_device_screenshot_with_runner(&request, &runner).unwrap();

        assert_eq!(result.connection.serial, "127.0.0.1:7555");
        assert_eq!(
            result.connection.selection_source,
            DeviceSelectionSource::ExistingConnectedDevice
        );
        assert_eq!(result.png_bytes, TEST_PNG);
        assert_eq!(
            runner.calls(),
            vec![
                vec!["start-server".to_string()],
                vec!["devices".to_string()],
                vec![
                    "-s".to_string(),
                    "127.0.0.1:7555".to_string(),
                    "exec-out".to_string(),
                    "screencap".to_string(),
                    "-p".to_string(),
                ],
            ]
        );
    }

    #[test]
    fn open_session_connects_candidate_when_nothing_is_connected() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(b"List of devices attached\n".to_vec()),
            FakeCommandResult::failure(1, b"cannot connect".to_vec(), b"".to_vec()),
            FakeCommandResult::success(b"already connected to 127.0.0.1:16384".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:16384\tdevice\n".to_vec(),
            ),
        ]);
        let request = DeviceConnectRequest {
            adb_executable: Some(PathBuf::from("adb")),
            preferred_serial: None,
            discovery_instance_count: 1,
        };

        let opened = open_device_session_with_runner(&request, &runner).unwrap();

        assert_eq!(opened.connection.serial, "127.0.0.1:16384");
        assert_eq!(
            opened.connection.selection_source,
            DeviceSelectionSource::ConnectedCandidate
        );
        assert_eq!(
            opened.connection.attempted_serials,
            vec!["127.0.0.1:7555".to_string(), "127.0.0.1:16384".to_string()]
        );
    }

    #[test]
    fn capture_retries_after_invalid_png_for_loopback_device() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:7555\tdevice\n".to_vec(),
            ),
            FakeCommandResult::success(b"not-a-png".to_vec()),
            FakeCommandResult::success(b"disconnected 127.0.0.1:7555".to_vec()),
            FakeCommandResult::success(b"connected to 127.0.0.1:7555".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:7555\tdevice\n".to_vec(),
            ),
            FakeCommandResult::success(TEST_PNG.to_vec()),
        ]);
        let request = ScreenshotCaptureRequest {
            adb_executable: Some(PathBuf::from("adb")),
            preferred_serial: Some("7555".to_string()),
            discovery_instance_count: 4,
        };

        let result = capture_device_screenshot_with_runner(&request, &runner).unwrap();

        assert_eq!(result.png_bytes, TEST_PNG);
        assert_eq!(result.connection.serial, "127.0.0.1:7555");
    }

    #[test]
    fn invalid_png_is_reported_for_non_loopback_devices() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\nemulator-5554\tdevice\n".to_vec(),
            ),
            FakeCommandResult::success(b"not-a-png".to_vec()),
        ]);
        let request = ScreenshotCaptureRequest {
            adb_executable: Some(PathBuf::from("adb.exe")),
            preferred_serial: Some("emulator-5554".to_string()),
            discovery_instance_count: 4,
        };

        let error = capture_device_screenshot_with_runner(&request, &runner).unwrap_err();
        assert!(matches!(error, ScreenshotCaptureError::InvalidPng));
    }

    #[test]
    fn send_keyevent_uses_existing_connected_device() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:7555\tdevice\n".to_vec(),
            ),
            FakeCommandResult::success(Vec::new()),
        ]);
        let request = DeviceInputRequest {
            adb_executable: Some(PathBuf::from("adb")),
            preferred_serial: None,
            discovery_instance_count: 4,
            action: DeviceInputAction::KeyEvent { key_code: 3 },
        };

        let result = send_device_input_with_runner(&request, &runner).unwrap();

        assert_eq!(result.connection.serial, "127.0.0.1:7555");
        assert_eq!(result.action, DeviceInputAction::KeyEvent { key_code: 3 });
        assert_eq!(
            runner.calls(),
            vec![
                vec!["start-server".to_string()],
                vec!["devices".to_string()],
                vec![
                    "-s".to_string(),
                    "127.0.0.1:7555".to_string(),
                    "shell".to_string(),
                    "input".to_string(),
                    "keyevent".to_string(),
                    "3".to_string(),
                ],
            ]
        );
    }

    #[test]
    fn send_tap_retries_after_loopback_failure() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:7555\tdevice\n".to_vec(),
            ),
            FakeCommandResult::failure(1, b"".to_vec(), b"tap failed".to_vec()),
            FakeCommandResult::success(b"disconnected 127.0.0.1:7555".to_vec()),
            FakeCommandResult::success(b"connected to 127.0.0.1:7555".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\n127.0.0.1:7555\tdevice\n".to_vec(),
            ),
            FakeCommandResult::success(Vec::new()),
        ]);
        let request = DeviceInputRequest {
            adb_executable: Some(PathBuf::from("adb")),
            preferred_serial: Some("7555".to_string()),
            discovery_instance_count: 4,
            action: DeviceInputAction::Tap { x: 120, y: 340 },
        };

        let result = send_device_input_with_runner(&request, &runner).unwrap();

        assert_eq!(result.connection.serial, "127.0.0.1:7555");
        assert_eq!(result.action, DeviceInputAction::Tap { x: 120, y: 340 });
    }

    #[test]
    fn send_swipe_reports_failure_for_non_loopback_without_retry() {
        let runner = FakeRunner::new(vec![
            FakeCommandResult::success(b"".to_vec()),
            FakeCommandResult::success(
                b"List of devices attached\nemulator-5554\tdevice\n".to_vec(),
            ),
            FakeCommandResult::failure(1, b"".to_vec(), b"swipe failed".to_vec()),
        ]);
        let request = DeviceInputRequest {
            adb_executable: Some(PathBuf::from("adb")),
            preferred_serial: Some("emulator-5554".to_string()),
            discovery_instance_count: 4,
            action: DeviceInputAction::Swipe {
                x1: 100,
                y1: 200,
                x2: 300,
                y2: 400,
                duration_ms: 500,
            },
        };

        let error = send_device_input_with_runner(&request, &runner).unwrap_err();
        match error {
            DeviceInputError::InputCommandFailed { action, details } => {
                assert_eq!(action, "滑动");
                assert!(details.contains("swipe failed"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    enum FakeCommandResult {
        Ok(CommandOutput),
    }

    impl FakeCommandResult {
        fn success(stdout: Vec<u8>) -> Self {
            Self::Ok(CommandOutput {
                status_code: 0,
                stdout,
                stderr: Vec::new(),
            })
        }

        fn failure(status_code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
            Self::Ok(CommandOutput {
                status_code,
                stdout,
                stderr,
            })
        }
    }

    struct FakeRunner {
        responses: RefCell<VecDeque<FakeCommandResult>>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeRunner {
        fn new(responses: Vec<FakeCommandResult>) -> Self {
            Self {
                responses: RefCell::new(responses.into()),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.borrow().clone()
        }
    }

    impl super::CommandRunner for FakeRunner {
        fn run(
            &self,
            _executable: &Path,
            args: &[String],
        ) -> Result<super::CommandOutput, io::Error> {
            self.calls.borrow_mut().push(args.to_vec());
            match self
                .responses
                .borrow_mut()
                .pop_front()
                .expect("fake response")
            {
                FakeCommandResult::Ok(output) => Ok(output),
            }
        }
    }
}
