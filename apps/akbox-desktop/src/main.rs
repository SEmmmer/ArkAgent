use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use akbox_core::config::AdbConfig;
use akbox_core::config::AppConfig;
use akbox_core::config::ConfigSource;
use akbox_core::config::DebugConfig;
use akbox_core::config::GameConfig;
use akbox_core::config::LoadedConfig;
use akbox_core::config::LoggingConfig;
use akbox_core::debug_artifact::DebugArtifactExportOutcome;
use akbox_core::debug_artifact::DebugArtifactExporter;
use akbox_core::debug_artifact::DebugArtifactKind;
use akbox_core::logging::init_logging;
use akbox_data::AppDatabase;
use akbox_data::AppRepository;
use akbox_data::ExternalEventNoticeRecord;
use akbox_data::ExternalItemDefRecord;
use akbox_data::ExternalOperatorBuildingSkillRecord;
use akbox_data::ExternalOperatorDefRecord;
use akbox_data::ExternalOperatorGrowthRecord;
use akbox_data::ExternalRecipeRecord;
use akbox_data::ExternalStageDefRecord;
use akbox_data::OFFICIAL_NOTICE_CACHE_KEY;
use akbox_data::OFFICIAL_NOTICE_SOURCE_ID;
use akbox_data::OfficialNoticeClient;
use akbox_data::PENGUIN_MATRIX_CACHE_KEY;
use akbox_data::PENGUIN_MATRIX_SOURCE_ID;
use akbox_data::PRTS_ITEM_INDEX_CACHE_KEY;
use akbox_data::PRTS_ITEM_INDEX_SOURCE_ID;
use akbox_data::PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY;
use akbox_data::PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID;
use akbox_data::PRTS_OPERATOR_GROWTH_CACHE_KEY;
use akbox_data::PRTS_OPERATOR_GROWTH_SOURCE_ID;
use akbox_data::PRTS_OPERATOR_INDEX_CACHE_KEY;
use akbox_data::PRTS_OPERATOR_INDEX_SOURCE_ID;
use akbox_data::PRTS_RECIPE_INDEX_CACHE_KEY;
use akbox_data::PRTS_RECIPE_INDEX_SOURCE_ID;
use akbox_data::PRTS_SITEINFO_CACHE_KEY;
use akbox_data::PRTS_SITEINFO_SOURCE_ID;
use akbox_data::PRTS_STAGE_INDEX_CACHE_KEY;
use akbox_data::PRTS_STAGE_INDEX_SOURCE_ID;
use akbox_data::PenguinClient;
use akbox_data::PenguinDropDisplayRecord;
use akbox_data::PrtsClient;
use akbox_data::RawSourceCacheSummary;
use akbox_data::SklandClient;
use akbox_data::SklandOperatorImportOutcome;
use akbox_data::SklandPlayerInfoInspectOutcome;
use akbox_data::SklandProfileRequest;
use akbox_data::SklandStatusBuildingImportOutcome;
use akbox_data::SyncMode;
use akbox_data::SyncOfficialNoticeOutcome;
use akbox_data::SyncPenguinMatrixOutcome;
use akbox_data::SyncPrtsOperatorBuildingSkillOutcome;
use akbox_data::SyncPrtsOperatorGrowthOutcome;
use akbox_data::SyncPrtsOutcome;
use akbox_data::SyncSourceStateRecord;
use akbox_data::default_database_path;
use akbox_data::import_skland_player_info_into_operator_state;
use akbox_data::import_skland_player_info_into_status_and_building_state;
use akbox_data::inspect_skland_player_info;
use akbox_data::sync_official_notices_with_mode;
use akbox_data::sync_penguin_matrix_with_mode;
use akbox_data::sync_prts_operator_building_skill_with_mode;
use akbox_data::sync_prts_operator_growth_with_mode;
use akbox_data::sync_prts_with_mode;
use akbox_device::DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT;
use akbox_device::DeviceConnectRequest;
use akbox_device::DeviceConnectionInfo;
use akbox_device::DeviceInputAction;
use akbox_device::DeviceInputRequest;
use akbox_device::DeviceInputResult;
use akbox_device::OcrRequest;
use akbox_device::PageConfirmationResult;
use akbox_device::RoiPurpose;
use akbox_device::ScreenshotCaptureRequest;
use akbox_device::ScreenshotCaptureResult;
use akbox_device::capture_device_screenshot;
use akbox_device::capture_device_screenshot_png;
use akbox_device::crop_all_rois_from_png;
use akbox_device::evaluate_page_confirmation_from_png;
use akbox_device::load_page_state_catalog_from_path;
use akbox_device::open_device_session;
use akbox_device::recognize_text_from_png;
use akbox_device::send_device_input;
use eframe::egui;
use qrcode::QrCode;
use qrcode::types::Color as QrColor;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

const SKLAND_APP_CODE: &str = "4ca99fa6b56cc2ba";
const SKLAND_USER_AGENT: &str =
    "Skland/1.32.1 (com.hypergryph.skland; build:103201004; Android 33; ) Okhttp/4.11.0";
const SKLAND_SCAN_POLL_INTERVAL: Duration = Duration::from_secs(2);
const SKLAND_SCAN_POLL_ATTEMPTS: usize = 90;
const SKLAND_QR_MODULE_SIZE: usize = 8;
const SKLAND_QR_QUIET_ZONE: usize = 4;

fn main() -> eframe::Result<()> {
    let bootstrap = DesktopBootstrap::load();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("方舟看号台"),
        ..Default::default()
    };

    eframe::run_native(
        "方舟看号台",
        options,
        Box::new(move |creation_context| {
            install_chinese_fonts(&creation_context.egui_ctx);
            Ok(Box::new(ArkAgentDesktopApp::new(bootstrap)))
        }),
    )
}

fn install_chinese_fonts(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "source_han_sans_sc_regular".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/SourceHanSansSC-Regular.otf"
        ))
        .into(),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "source_han_sans_sc_regular".to_owned());
    }

    context.set_fonts(fonts);
}

struct DesktopBootstrap {
    loaded_config: LoadedConfig,
    active_log_file: Option<PathBuf>,
    working_directory: PathBuf,
    startup_notices: Vec<UiNotice>,
}

impl DesktopBootstrap {
    fn load() -> Self {
        let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let fallback_config_path = AppConfig::default_config_path()
            .unwrap_or_else(|_| PathBuf::from(akbox_core::config::DEFAULT_CONFIG_FILE_NAME));

        let (loaded_config, config_notice) = match AppConfig::load() {
            Ok(loaded) => (loaded, None),
            Err(error) => (
                LoadedConfig {
                    source: ConfigSource::Defaults {
                        expected_path: fallback_config_path,
                    },
                    config: AppConfig::default(),
                },
                Some(UiNotice::warning(format!(
                    "启动时读取配置失败：{error}。当前先使用默认配置，保存后会生成本地配置文件。"
                ))),
            ),
        };

        let (active_log_file, logging_notice) = match init_logging(&loaded_config.config) {
            Ok(state) => {
                tracing::info!(log_file = %state.log_file.display(), "desktop app starting");
                (Some(state.log_file.clone()), None)
            }
            Err(error) => (
                None,
                Some(UiNotice::error(format!("初始化文件日志失败：{error}"))),
            ),
        };

        let mut startup_notices = Vec::new();
        if let Some(notice) = config_notice {
            startup_notices.push(notice);
        }
        if let Some(notice) = logging_notice {
            startup_notices.push(notice);
        }

        Self {
            loaded_config,
            active_log_file,
            working_directory,
            startup_notices,
        }
    }
}

struct ArkAgentDesktopApp {
    current_page: Page,
    working_directory: PathBuf,
    active_log_file: Option<PathBuf>,
    startup_notices: Vec<UiNotice>,
    settings: SettingsPageState,
    device: DevicePageState,
    vision: VisionDebugPageState,
    sync: SyncPageState,
    skland_login: SklandLoginPageState,
}

impl ArkAgentDesktopApp {
    fn new(bootstrap: DesktopBootstrap) -> Self {
        Self {
            current_page: Page::Dashboard,
            working_directory: bootstrap.working_directory.clone(),
            active_log_file: bootstrap.active_log_file,
            startup_notices: bootstrap.startup_notices,
            settings: SettingsPageState::from_loaded(&bootstrap.loaded_config),
            device: DevicePageState::new(),
            vision: VisionDebugPageState::new(&bootstrap.working_directory),
            sync: SyncPageState::new(&bootstrap.working_directory),
            skland_login: SklandLoginPageState::new(&bootstrap.working_directory),
        }
    }

    fn write_test_log_entry(&mut self) {
        let adb = self.settings.form.adb_executable.trim();

        tracing::info!(
            adb_executable = adb,
            timezone = %self.settings.form.game_timezone,
            export_artifacts = self.settings.form.export_artifacts,
            "desktop settings test log entry"
        );

        let log_target = self
            .active_log_file
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<日志不可用>".to_string());
        self.settings.notice = Some(UiNotice::success(format!(
            "已向 {log_target} 写入一条测试日志"
        )));
    }

    fn export_real_screenshot(&mut self) {
        let config = self.settings.form.to_config();
        let exporter = DebugArtifactExporter::from_config(&config, &self.working_directory);

        if !config.debug.export_artifacts {
            self.settings.notice = Some(UiNotice::warning(format!(
                "真实截图导出入口已预留，但当前未启用“导出调试产物”。启用后，阶段 4 会把设备截图导出到 {}。",
                exporter.export_directory().display()
            )));
            return;
        }

        let request = match self
            .device
            .build_capture_request(config.adb.executable.clone())
        {
            Ok(request) => request,
            Err(error) => {
                self.settings.notice = Some(UiNotice::error(error));
                return;
            }
        };

        match capture_device_screenshot_png(&request) {
            Ok(png_bytes) => match exporter.export_bytes(
                DebugArtifactKind::Screenshot,
                "desktop-device-capture",
                "png",
                &png_bytes,
            ) {
                Ok(DebugArtifactExportOutcome::Exported(file)) => {
                    tracing::info!(
                        screenshot_path = %file.path.display(),
                        "desktop real screenshot exported"
                    );
                    self.settings.notice = Some(UiNotice::success(format!(
                        "已导出真实设备截图到 {}",
                        file.path.display()
                    )));
                }
                Ok(DebugArtifactExportOutcome::Disabled { directory }) => {
                    self.settings.notice = Some(UiNotice::warning(format!(
                        "真实截图导出已触发，但当前目录 {} 被标记为禁用。",
                        directory.display()
                    )));
                }
                Err(error) => {
                    tracing::error!(%error, "failed to write exported real screenshot");
                    self.settings.notice =
                        Some(UiNotice::error(format!("写入真实截图失败：{error}")));
                }
            },
            Err(error) => {
                tracing::info!(
                    export_directory = %exporter.export_directory().display(),
                    %error,
                    "desktop real screenshot export entry invoked before backend is ready"
                );
                self.settings.notice = Some(UiNotice::warning(format!(
                    "真实截图导出入口已就位，但当前还不能抓取设备画面：{error}。阶段 4 完成后，这里会把 MuMu 当前画面导出到 {}。",
                    exporter.export_directory().display()
                )));
            }
        }
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("仪表盘");
        ui.label("当前已有设备接入、视觉调试、外部同步、可选森空岛登录和本地配置几个可操作入口。");
        ui.separator();
        ui.label(format!("工作目录：{}", self.working_directory.display()));
        ui.label(format!("配置来源：{}", self.settings.source_description_zh));
        ui.label(format!("配置路径：{}", self.settings.config_path.display()));
        ui.label(format!(
            "当前日志文件：{}",
            self.active_log_file
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<日志不可用>".to_string())
        ));
        ui.label(format!(
            "同步数据库：{}",
            self.sync.database_path_text.trim()
        ));
        ui.label(format!("PRTS 站点状态：{}", self.sync.prts.status_label()));
        ui.label(format!("设备状态：{}", self.device.status_label()));
        ui.label(format!(
            "PRTS 道具状态：{}",
            self.sync.prts_items.source.status_label()
        ));
        ui.label(format!(
            "PRTS 干员状态：{}",
            self.sync.prts_operators.source.status_label()
        ));
        ui.label(format!(
            "PRTS 基建技能状态：{}",
            self.sync.prts_building_skills.source.status_label()
        ));
        ui.label(format!(
            "官方公告状态：{}",
            self.sync.official.source.status_label()
        ));
        ui.label(format!(
            "Penguin 状态：{}",
            self.sync.penguin.source.status_label()
        ));
        ui.label(format!(
            "森空岛本地鉴权：{}",
            self.skland_login.local_auth_status_label()
        ));
        ui.label("截图来源：MuMu / ADB 设备截图（阶段 4 接入后可用）");
        ui.separator();

        if ui.button("打开同步页").clicked() {
            self.current_page = Page::Sync;
        }
        if ui.button("打开设备页").clicked() {
            self.current_page = Page::Device;
        }
        if ui.button("打开视觉调试页").clicked() {
            self.current_page = Page::Vision;
        }
        if ui.button("打开森空岛登录页").clicked() {
            self.current_page = Page::SklandLogin;
        }
        if ui.button("打开设置页").clicked() {
            self.current_page = Page::Settings;
        }
        if ui.button("写入测试日志").clicked() {
            self.write_test_log_entry();
        }
        if ui.button("导出真实截图").clicked() {
            self.export_real_screenshot();
        }
    }

    fn render_sync(&mut self, ui: &mut egui::Ui) {
        ui.heading("同步");
        ui.label("这里展示 PRTS、官方公告与 Penguin 的本地同步状态，并允许触发后台同步。");
        ui.separator();

        let running = self.sync.is_running();
        ui.horizontal(|ui| {
            ui.label("数据库路径");
            ui.text_edit_singleline(&mut self.sync.database_path_text);
        });

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.sync.force_full_sync, "全部同步");
            ui.label("关闭时默认增量同步；勾选后强制全量同步。");
        });
        ui.label(
            "当前“同步 PRTS”只覆盖站点 / 干员 / 道具 / 关卡 / 配方；其中干员 / 道具 / 关卡 / 配方，以及 Penguin 支持轻量版本锚点预检查。官方公告、PRTS 养成需求和 PRTS 基建技能都来自列表页或单干员 section，缺少稳定增量锚点，因此即使请求增量也仍按全量执行。",
        );

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("刷新本地状态"))
                .clicked()
            {
                match self.sync.refresh_from_database() {
                    Ok(()) => {
                        self.sync.notice = Some(UiNotice::success("已刷新本地同步概览"));
                    }
                    Err(error) => {
                        self.sync.notice =
                            Some(UiNotice::error(format!("刷新本地同步概览失败：{error}")));
                    }
                }
            }

            if ui
                .add_enabled(!running, egui::Button::new("同步 PRTS"))
                .clicked()
            {
                self.sync.start_prts_sync(&self.working_directory);
            }

            if ui
                .add_enabled(!running, egui::Button::new("同步 PRTS 养成需求"))
                .clicked()
            {
                self.sync.start_prts_growth_sync();
            }

            if ui
                .add_enabled(!running, egui::Button::new("同步 PRTS 基建技能"))
                .clicked()
            {
                self.sync.start_prts_building_skill_sync();
            }

            if ui
                .add_enabled(!running, egui::Button::new("同步官方公告"))
                .clicked()
            {
                self.sync.start_official_sync();
            }

            if ui
                .add_enabled(!running, egui::Button::new("同步 Penguin"))
                .clicked()
            {
                self.sync.start_penguin_sync();
            }

            if let Some(label) = self.sync.running_label() {
                ui.colored_label(egui::Color32::from_rgb(80, 120, 176), label);
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.sync.selected_tab, SyncTab::Prts, "PRTS");
            ui.selectable_value(&mut self.sync.selected_tab, SyncTab::Official, "官方公告");
            ui.selectable_value(&mut self.sync.selected_tab, SyncTab::Penguin, "Penguin");
        });
        ui.separator();

        match self.sync.selected_tab {
            SyncTab::Prts => render_prts_overview(
                ui,
                PrtsOverviewRefs {
                    site_info: &self.sync.prts,
                    item_index: &self.sync.prts_items,
                    operator_index: &self.sync.prts_operators,
                    operator_building_skill: &self.sync.prts_building_skills,
                    operator_growth: &self.sync.prts_growth,
                    recipe_index: &self.sync.prts_recipes,
                    stage_index: &self.sync.prts_stages,
                },
                self.settings.form.game_timezone.as_str(),
            ),
            SyncTab::Official => render_official_notice_overview(
                ui,
                &self.sync.official,
                self.settings.form.game_timezone.as_str(),
            ),
            SyncTab::Penguin => render_penguin_overview(
                ui,
                &self.sync.penguin,
                self.settings.form.game_timezone.as_str(),
            ),
        }
    }

    fn render_device(&mut self, ui: &mut egui::Ui) {
        ui.heading("设备");
        ui.label("这里负责 MuMu / ADB 设备发现、连接状态与真实截图预览，是 M4 的主页面。");
        ui.separator();

        let running = self.device.is_running();
        ui.label(
            "自动发现顺序：先检查已连接的 `127.0.0.1:7555` 与 `16384 + 32 * n`（n 从 0 开始），找不到时再按同顺序尝试 `adb connect`。手动指定串号/端口后，只会尝试该目标。",
        );
        ui.label(
            "ADB 路径优先读取设置页当前值；若留空，则会尝试 PATH 和常见 MuMu 安装目录中的 `adb.exe`。",
        );
        ui.separator();

        egui::Grid::new("device_form")
            .num_columns(2)
            .spacing([24.0, 12.0])
            .show(ui, |ui| {
                ui.label("手动串号 / 端口");
                ui.text_edit_singleline(&mut self.device.preferred_serial_text);
                ui.end_row();

                ui.label("自动探测实例数");
                ui.text_edit_singleline(&mut self.device.discovery_instance_count_text);
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("刷新设备连接"))
                .clicked()
            {
                self.device
                    .start_connect(self.settings.form.to_config().adb.executable.clone());
            }

            if ui
                .add_enabled(!running, egui::Button::new("抓取截图预览"))
                .clicked()
            {
                self.device
                    .start_capture(self.settings.form.to_config().adb.executable.clone());
            }

            if ui.button("清空预览").clicked() {
                self.device.clear_preview();
            }

            if let Some(label) = self.device.running_label() {
                ui.colored_label(egui::Color32::from_rgb(80, 120, 176), label);
            }
        });

        ui.separator();
        ui.label(format!("当前设备状态：{}", self.device.status_label()));

        if let Some(connection) = self.device.connection.as_ref() {
            ui.label(format!("当前 adb：{}", connection.adb_executable.display()));
            ui.label(format!("当前设备串号：{}", connection.serial));
            ui.label(format!(
                "连接来源：{}",
                connection.selection_source.label_zh()
            ));
            ui.label(format!(
                "本轮尝试的端点：{}",
                if connection.attempted_serials.is_empty() {
                    "<无>".to_string()
                } else {
                    connection.attempted_serials.join(", ")
                }
            ));
            ui.label(format!(
                "当前 adb devices：{}",
                render_adb_devices_summary(&connection.visible_devices)
            ));
        } else {
            ui.label("当前还没有成功建立设备连接。");
        }

        if let Some(dimensions) = self.device.preview_dimensions {
            ui.label(format!(
                "最近截图尺寸：{} x {}；PNG 大小：{} 字节；抓取时间：{}",
                dimensions[0],
                dimensions[1],
                self.device.preview_bytes_len.unwrap_or_default(),
                self.device.preview_loaded_at.as_deref().unwrap_or("<未知>")
            ));
        } else {
            ui.label("当前还没有截图预览。");
        }

        ui.separator();
        ui.label("输入动作测试：");
        ui.label("这些按钮会直接向当前设备发送真实输入，请在确认当前画面状态后再使用。");

        ui.group(|ui| {
            ui.label("点按测试");
            egui::Grid::new("device_tap_input_form")
                .num_columns(5)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label("点按 X");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.tap_x_text),
                    );
                    ui.label("点按 Y");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.tap_y_text),
                    );
                    if ui
                        .add_enabled(!running, egui::Button::new("发送点按"))
                        .clicked()
                    {
                        match self.device.tap_action() {
                            Ok(action) => self.device.start_input_action(
                                self.settings.form.to_config().adb.executable.clone(),
                                action,
                            ),
                            Err(error) => self.device.notice = Some(UiNotice::error(error)),
                        }
                    }
                    ui.end_row();
                });
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("滑动测试");
            egui::Grid::new("device_swipe_input_form")
                .num_columns(4)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label("起点 X");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.swipe_x1_text),
                    );
                    ui.label("起点 Y");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.swipe_y1_text),
                    );
                    ui.end_row();

                    ui.label("终点 X");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.swipe_x2_text),
                    );
                    ui.label("终点 Y");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.swipe_y2_text),
                    );
                    ui.end_row();

                    ui.label("时长 ms");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.swipe_duration_ms_text),
                    );
                    if ui
                        .add_enabled(!running, egui::Button::new("发送滑动"))
                        .clicked()
                    {
                        match self.device.swipe_action() {
                            Ok(action) => self.device.start_input_action(
                                self.settings.form.to_config().adb.executable.clone(),
                                action,
                            ),
                            Err(error) => self.device.notice = Some(UiNotice::error(error)),
                        }
                    }
                    ui.end_row();
                });
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("按键测试");
            egui::Grid::new("device_keyevent_input_form")
                .num_columns(5)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label("按键码");
                    ui.add_sized(
                        [72.0, 0.0],
                        egui::TextEdit::singleline(&mut self.device.keyevent_code_text),
                    );
                    ui.label("常用");
                    ui.label("3=HOME 4=BACK 26=POWER");
                    if ui
                        .add_enabled(!running, egui::Button::new("发送按键"))
                        .clicked()
                    {
                        match self.device.keyevent_action() {
                            Ok(action) => self.device.start_input_action(
                                self.settings.form.to_config().adb.executable.clone(),
                                action,
                            ),
                            Err(error) => self.device.notice = Some(UiNotice::error(error)),
                        }
                    }
                    ui.end_row();
                });
        });

        ui.separator();
        if let Some(texture) = self.device.preview_texture.as_ref() {
            let available_width = ui.available_width().max(320.0);
            let image_size = texture.size_vec2();
            let scale = (available_width / image_size.x).min(1.0);
            ui.add(
                egui::Image::from_texture(texture).fit_to_exact_size(image_size * scale.max(0.1)),
            );
        } else {
            ui.label("抓取成功后，这里会显示 MuMu 当前真实画面。");
        }
    }

    fn render_vision(&mut self, ui: &mut egui::Ui) {
        ui.heading("视觉调试");
        ui.label(
            "这里用于验证页面模板、当前设备截图或本地 PNG 是否能被正确识别，是 M5 的可视化调试入口。",
        );
        ui.label("当前仍是调试页，不会直接写入仓库/干员最终状态。");
        ui.separator();

        let running = self.vision.is_running();
        let device_busy = self.device.is_running();
        let can_run = !(running
            || (device_busy
                && self.vision.input_source_mode
                    == VisionInputSourceMode::CurrentDeviceScreenshot));

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("刷新页面模板"))
                .clicked()
            {
                match self.vision.refresh_page_catalog(&self.working_directory) {
                    Ok(()) => {
                        let message = if self.vision.available_pages.is_empty() {
                            "已刷新页面模板，但当前仍未发现可用页面配置"
                        } else {
                            "已刷新页面模板列表"
                        };
                        self.vision.notice = Some(UiNotice::success(message));
                    }
                    Err(error) => {
                        self.vision.notice =
                            Some(UiNotice::error(format!("刷新页面模板失败：{error}")));
                    }
                }
            }

            if let Some(label) = self.vision.running_label() {
                ui.colored_label(egui::Color32::from_rgb(80, 120, 176), label);
            }
        });

        if self.vision.available_pages.is_empty() {
            ui.colored_label(
                egui::Color32::YELLOW,
                "当前还没有发现可用页面模板。后续放入 `assets/templates/pages/*.json` 后，这里会自动列出。",
            );
        } else {
            let current_key = self.vision.selected_page_key.clone();
            let mut next_key = current_key.clone();
            let page_options = self
                .vision
                .available_pages
                .iter()
                .map(|page| (page.key.clone(), page.user_label()))
                .collect::<Vec<_>>();

            egui::ComboBox::from_label("调试页面")
                .width(320.0)
                .selected_text(self.vision.selected_page_label())
                .show_ui(ui, |ui| {
                    for (key, label) in &page_options {
                        ui.selectable_value(&mut next_key, Some(key.clone()), label);
                    }
                });

            if next_key != current_key {
                self.vision.set_selected_page_key(next_key);
            }

            if let Some(page) = self.vision.selected_page() {
                ui.label(format!(
                    "当前页面：{}；确认特征 {} 个；ROI {} 个",
                    page.user_label(),
                    page.marker_count,
                    page.roi_count
                ));
            }
        }

        ui.separator();
        ui.label("图像来源");
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.vision.input_source_mode,
                VisionInputSourceMode::CurrentDeviceScreenshot,
                "使用当前设备截图",
            );
            ui.selectable_value(
                &mut self.vision.input_source_mode,
                VisionInputSourceMode::LocalPng,
                "使用本地 PNG",
            );
        });

        match self.vision.input_source_mode {
            VisionInputSourceMode::CurrentDeviceScreenshot => {
                ui.label(format!("当前设备状态：{}", self.device.status_label()));
                ui.label("运行时会直接通过 MuMu / ADB 抓取当前画面，再立即执行页面确认、ROI 裁剪和 OCR。");
                if device_busy {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "设备页当前也有后台任务，需等待其完成后再抓取当前设备截图。",
                    );
                }
            }
            VisionInputSourceMode::LocalPng => {
                ui.horizontal(|ui| {
                    ui.label("本地 PNG 路径");
                    ui.text_edit_singleline(&mut self.vision.input_png_path_text);
                });
                ui.label("适合离线调试已有截图；不会访问当前设备。");
            }
        }

        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_run, egui::Button::new(self.vision.run_button_label()))
                .clicked()
            {
                let capture_request = if self.vision.input_source_mode
                    == VisionInputSourceMode::CurrentDeviceScreenshot
                {
                    match self.device.build_capture_request(
                        self.settings.form.to_config().adb.executable.clone(),
                    ) {
                        Ok(request) => Some(request),
                        Err(error) => {
                            self.vision.notice = Some(UiNotice::error(error));
                            None
                        }
                    }
                } else {
                    None
                };

                if capture_request.is_some()
                    || self.vision.input_source_mode == VisionInputSourceMode::LocalPng
                {
                    self.vision
                        .start_inspect(&self.working_directory, capture_request);
                }
            }

            if ui.button("清空结果").clicked() {
                self.vision.clear_result();
            }
        });

        egui::CollapsingHeader::new("高级选项")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("这些字段主要用于模板维护和故障定位，普通使用通常不需要修改。");
                egui::Grid::new("vision_debug_advanced_form")
                    .num_columns(2)
                    .spacing([24.0, 12.0])
                    .show(ui, |ui| {
                        ui.label("页面配置路径");
                        ui.text_edit_singleline(&mut self.vision.page_config_path_text);
                        ui.end_row();

                        ui.label("页面 ID");
                        ui.text_edit_singleline(&mut self.vision.page_id_text);
                        ui.end_row();

                        ui.label("模板根目录");
                        ui.text_edit_singleline(&mut self.vision.templates_root_path_text);
                        ui.end_row();

                        ui.label("输出目录");
                        ui.text_edit_singleline(&mut self.vision.output_dir_path_text);
                        ui.end_row();
                    });

                ui.label(format!(
                    "解析后的模板根目录：{}",
                    self.vision
                        .resolved_templates_root(&self.working_directory)
                        .display()
                ));
                ui.label(format!(
                    "解析后的输出目录：{}",
                    self.vision
                        .resolved_output_dir(&self.working_directory)
                        .display()
                ));
            });

        ui.separator();
        if let Some(result) = self.vision.last_result.as_ref() {
            ui.label(format!(
                "最近调试页面：{}（page_id = {}）",
                result.page_display_name, result.page_id
            ));
            ui.label(format!("图像来源：{}", result.source_label));
            ui.label(format!("页面配置：{}", result.page_config_path.display()));
            ui.label(format!("输入 PNG：{}", result.input_png_path.display()));
            ui.label(format!("Manifest：{}", result.manifest_path.display()));
            ui.label(format!(
                "页面判断：{}（{}/{} 个 marker 通过）",
                if result.page_confirmation.matched {
                    "已识别为目标页面"
                } else {
                    "未能稳定确认是目标页面"
                },
                result.page_confirmation.matched_markers,
                result.page_confirmation.total_markers
            ));
            if !result.page_confirmation.matched {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "若你确定画面无误，通常意味着模板需要更新，或当前截图并不处在该页面。",
                );
            }
            ui.label(format!("输出目录：{}", result.output_dir.display()));

            if let Some(dimensions) = self.vision.source_dimensions {
                ui.label(format!(
                    "源图尺寸：{} x {}；PNG 大小：{} 字节",
                    dimensions[0],
                    dimensions[1],
                    self.vision.source_bytes_len.unwrap_or_default()
                ));
            }

            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label("确认特征结果");
                if result.page_confirmation.marker_results.is_empty() {
                    ui.label("当前页面没有配置 confirmation markers。");
                } else {
                    egui::Grid::new("vision_marker_results")
                        .num_columns(6)
                        .spacing([16.0, 8.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Marker");
                            ui.strong("策略");
                            ui.strong("得分");
                            ui.strong("阈值");
                            ui.strong("通过");
                            ui.strong("备注");
                            ui.end_row();

                            for marker in &result.page_confirmation.marker_results {
                                ui.label(marker.marker_id.as_str());
                                ui.label(describe_confirmation_strategy(marker.strategy));
                                ui.label(optional_score(marker.score));
                                ui.label(optional_score(marker.pass_threshold));
                                ui.label(if marker.passed { "是" } else { "否" });
                                ui.label(marker.note.as_deref().unwrap_or("-"));
                                ui.end_row();
                            }
                        });
                }
            });

            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label("ROI 输出");
                if result.roi_outputs.is_empty() {
                    ui.label("当前页面没有配置 ROI。");
                } else {
                    egui::Grid::new("vision_roi_results")
                        .num_columns(5)
                        .spacing([16.0, 8.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("ROI");
                            ui.strong("用途");
                            ui.strong("OCR");
                            ui.strong("说明");
                            ui.strong("输出 PNG");
                            ui.end_row();

                            for roi in &result.roi_outputs {
                                ui.label(format!("{} / {}", roi.roi_id, roi.display_name));
                                ui.label(describe_roi_purpose(roi.purpose));
                                ui.label(roi.ocr_status.as_str());
                                ui.label(roi.ocr_message.as_str());
                                ui.label(roi.output_png_path.display().to_string());
                                ui.end_row();
                            }
                        });
                }
            });

            ui.add_space(8.0);
            if let Some(texture) = self.vision.source_texture.as_ref() {
                let available_width = ui.available_width().max(320.0);
                let image_size = texture.size_vec2();
                let scale = (available_width / image_size.x).min(1.0);
                ui.label("源图预览");
                ui.add(
                    egui::Image::from_texture(texture)
                        .fit_to_exact_size(image_size * scale.max(0.1)),
                );
            } else {
                ui.label("本轮调试没有可显示的源图预览。");
            }
        } else {
            ui.label("运行成功后，这里会显示页面判断、ROI 输出、OCR 结果和源图预览。");
        }
    }

    fn render_skland_login(&mut self, ui: &mut egui::Ui) {
        ui.heading("森空岛登录");
        ui.label("这里提供森空岛扫码登录、player/info 调试与干员状态导入。当前森空岛作为干员当前态的高优先级数据源，OCR 退到校验与兜底。");
        ui.separator();

        let running = self.skland_login.is_running();
        ui.horizontal(|ui| {
            ui.label("本地鉴权文件");
            ui.add_enabled_ui(!running, |ui| {
                ui.text_edit_singleline(&mut self.skland_login.auth_file_path_text);
            });
        });
        ui.horizontal(|ui| {
            ui.label("数据库路径");
            ui.add_enabled_ui(!running, |ui| {
                ui.text_edit_singleline(&mut self.skland_login.database_path_text);
            });
        });
        ui.label(format!(
            "解析后的数据库路径：{}",
            self.skland_login
                .resolved_database_path(&self.working_directory)
                .display()
        ));
        ui.horizontal(|ui| {
            ui.label("uid_file 字段");
            ui.add_enabled_ui(!running, |ui| {
                ui.text_edit_singleline(&mut self.skland_login.uid_file_text);
            });
        });
        ui.label(format!(
            "解析后的鉴权文件路径：{}",
            self.skland_login
                .resolved_auth_file_path(&self.working_directory)
                .display()
        ));
        ui.small(
            "如果文件还不存在，扫码成功后会自动创建并写入 access_token / cred / token / user_id。",
        );

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("确认登录"))
                .clicked()
            {
                self.skland_login.start_login(&self.working_directory);
            }
            if ui
                .add_enabled(!running, egui::Button::new("检查 player/info"))
                .clicked()
            {
                self.skland_login
                    .start_player_info_inspect(&self.working_directory);
            }
            if ui
                .add_enabled(!running, egui::Button::new("导入账号/基建状态"))
                .clicked()
            {
                self.skland_login
                    .start_status_building_import(&self.working_directory);
            }
            if ui
                .add_enabled(!running, egui::Button::new("导入到干员状态"))
                .clicked()
            {
                self.skland_login
                    .start_operator_state_import(&self.working_directory);
            }
            if ui.button("刷新本地状态").clicked() {
                match self
                    .skland_login
                    .refresh_local_auth_status(&self.working_directory)
                {
                    Ok(message) => self.skland_login.notice = Some(UiNotice::info(message)),
                    Err(error) => self.skland_login.notice = Some(UiNotice::error(error)),
                }
            }
            if self.skland_login.can_reopen_qr_popup() && ui.button("显示当前二维码").clicked()
            {
                self.skland_login.show_qr_popup = true;
            }
        });

        if let Some(label) = self.skland_login.running_label() {
            ui.label(label);
        }
        ui.label(format!("当前状态：{}", self.skland_login.status_label()));

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("本地鉴权文件状态");
            if let Some(summary) = self.skland_login.local_auth_summary.as_ref() {
                egui::Grid::new("skland_auth_file_summary")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("文件路径");
                        ui.label(summary.auth_file_path.display().to_string());
                        ui.end_row();

                        ui.label("文件状态");
                        ui.label(if summary.exists {
                            "已存在"
                        } else {
                            "尚未创建"
                        });
                        ui.end_row();

                        ui.label("uid_file");
                        ui.label(summary.uid_file.as_str());
                        ui.end_row();

                        ui.label("access_token");
                        ui.label(if summary.has_access_token {
                            "已写入"
                        } else {
                            "未写入"
                        });
                        ui.end_row();

                        ui.label("cred");
                        ui.label(if summary.has_cred {
                            "已写入"
                        } else {
                            "未写入"
                        });
                        ui.end_row();

                        ui.label("token");
                        ui.label(if summary.has_token {
                            "已写入"
                        } else {
                            "未写入"
                        });
                        ui.end_row();

                        ui.label("user_id");
                        ui.label(if summary.has_user_id {
                            "已写入"
                        } else {
                            "未写入"
                        });
                        ui.end_row();
                    });
            } else {
                ui.label("当前还没有可显示的本地鉴权文件状态。");
            }
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("player/info 摘要");
            if let Some(summary) = self.skland_login.last_player_info_inspect.as_ref() {
                egui::Grid::new("skland_player_info_summary")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("版本锚点");
                        ui.label(summary.revision.as_str());
                        ui.end_row();

                        ui.label("UID");
                        ui.label(summary.uid.as_str());
                        ui.end_row();

                        ui.label("账号昵称");
                        ui.label(summary.account_name.as_deref().unwrap_or("<未知>"));
                        ui.end_row();

                        ui.label("storeTs");
                        ui.label(optional_integer(summary.status_store_ts));
                        ui.end_row();

                        ui.label("状态字段");
                        ui.label(if summary.status_keys.is_empty() {
                            "<无>".to_string()
                        } else {
                            summary.status_keys.join(", ")
                        });
                        ui.end_row();

                        ui.label("绑定角色");
                        ui.label(summary.binding_count.to_string());
                        ui.end_row();

                        ui.label("干员数量");
                        ui.label(summary.char_count.to_string());
                        ui.end_row();

                        ui.label("助战数量");
                        ui.label(summary.assist_count.to_string());
                        ui.end_row();

                        ui.label("equipmentInfoMap");
                        ui.label(summary.equipment_info_count.to_string());
                        ui.end_row();

                        ui.label("charInfoMap");
                        ui.label(summary.char_info_count.to_string());
                        ui.end_row();

                        ui.label("基建字段");
                        ui.label(if summary.has_building {
                            summary.building_keys.join(", ")
                        } else {
                            "<无>".to_string()
                        });
                        ui.end_row();

                        ui.label("基建摘要");
                        ui.label(format!(
                            "control={} / meeting={} / training={} / hire={} / dorm={} / manuf={} / trade={} / power={} / tired={}",
                            bool_text(summary.has_control),
                            bool_text(summary.has_meeting),
                            bool_text(summary.has_training),
                            bool_text(summary.has_hire),
                            summary.dormitory_count,
                            summary.manufacture_count,
                            summary.trading_count,
                            summary.power_count,
                            summary.tired_char_count
                        ));
                        ui.end_row();

                        ui.label("样例干员");
                        ui.label(render_skland_sample_operator_summary(
                            summary.sample_operator.as_ref(),
                        ));
                        ui.end_row();

                        ui.label("缓存字节数");
                        ui.label(summary.cache_size_bytes.to_string());
                        ui.end_row();
                    });
            } else {
                ui.label("尚未执行 player/info 检查。");
            }
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("最近一次导入结果");
            if let Some(import) = self.skland_login.last_operator_import.as_ref() {
                egui::Grid::new("skland_operator_import_summary")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("快照 ID");
                        ui.label(import.snapshot_id.as_str());
                        ui.end_row();

                        ui.label("导入行数");
                        ui.label(import.imported_row_count.to_string());
                        ui.end_row();

                        ui.label("已持有");
                        ui.label(import.owned_row_count.to_string());
                        ui.end_row();

                        ui.label("未持有");
                        ui.label(import.unowned_row_count.to_string());
                        ui.end_row();

                        ui.label("使用外部干员定义");
                        ui.label(if import.used_external_operator_defs {
                            "是"
                        } else {
                            "否"
                        });
                        ui.end_row();
                    });
            } else {
                ui.label("尚未执行导入。");
            }
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("最近一次账号/基建导入");
            if let Some(import) = self.skland_login.last_status_building_import.as_ref() {
                egui::Grid::new("skland_status_building_import_summary")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("账号状态快照");
                        ui.label(import.player_status_snapshot_id.as_str());
                        ui.end_row();

                        ui.label("基建快照");
                        ui.label(import.base_building_snapshot_id.as_str());
                        ui.end_row();

                        ui.label("账号昵称");
                        ui.label(import.inspect.account_name.as_deref().unwrap_or("<未知>"));
                        ui.end_row();

                        ui.label("状态字段");
                        ui.label(if import.inspect.status_keys.is_empty() {
                            "<无>".to_string()
                        } else {
                            import.inspect.status_keys.join(", ")
                        });
                        ui.end_row();

                        ui.label("基建摘要");
                        ui.label(format!(
                            "control={} / meeting={} / training={} / hire={} / dorm={} / manuf={} / trade={} / power={} / tired={}",
                            bool_text(import.inspect.has_control),
                            bool_text(import.inspect.has_meeting),
                            bool_text(import.inspect.has_training),
                            bool_text(import.inspect.has_hire),
                            import.inspect.dormitory_count,
                            import.inspect.manufacture_count,
                            import.inspect.trading_count,
                            import.inspect.power_count,
                            import.inspect.tired_char_count
                        ));
                        ui.end_row();
                    });
            } else {
                ui.label("尚未执行账号/基建导入。");
            }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.label("这里已经可以读取、编辑、保存并验证本地配置，是当前阶段的主要实操页面。");
        ui.separator();

        ui.label(format!("配置来源：{}", self.settings.source_description_zh));
        ui.label(format!("保存目标：{}", self.settings.config_path.display()));
        ui.label(format!(
            "当前日志文件：{}",
            self.active_log_file
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<日志不可用>".to_string())
        ));
        ui.label(
            "真实截图将通过 MuMu / ADB 设备截图链路导出；当前阶段先提供入口，不再导出占位样例图。",
        );
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("从磁盘重载").clicked() {
                match self.settings.reload() {
                    Ok(message) => {
                        tracing::info!("desktop config reloaded from disk");
                        self.settings.notice = Some(UiNotice::success(message));
                    }
                    Err(error) => {
                        tracing::error!(%error, "failed to reload config from disk");
                        self.settings.notice =
                            Some(UiNotice::error(format!("从磁盘重载配置失败：{error}")));
                    }
                }
            }

            if ui.button("保存到配置文件").clicked() {
                match self.settings.save() {
                    Ok(outcome) => {
                        tracing::info!(path = %outcome.path.display(), "desktop config saved");
                        let mut message = format!("已将配置保存到 {}", outcome.path.display());
                        if outcome.logging_changed {
                            message.push_str("。日志路径变更将在应用重启后生效。");
                        }
                        self.settings.notice = Some(UiNotice::success(message));
                    }
                    Err(error) => {
                        tracing::error!(%error, "failed to save config from desktop");
                        self.settings.notice =
                            Some(UiNotice::error(format!("保存配置失败：{error}")));
                    }
                }
            }

            if ui.button("写入测试日志").clicked() {
                self.write_test_log_entry();
            }

            if ui.button("导出真实截图").clicked() {
                self.export_real_screenshot();
            }
        });

        ui.separator();
        egui::Grid::new("settings_form")
            .num_columns(2)
            .spacing([24.0, 12.0])
            .show(ui, |ui| {
                ui.label("ADB 程序路径");
                ui.text_edit_singleline(&mut self.settings.form.adb_executable);
                ui.end_row();

                ui.label("游戏时区");
                ui.text_edit_singleline(&mut self.settings.form.game_timezone);
                ui.end_row();

                ui.label("日志目录");
                ui.text_edit_singleline(&mut self.settings.form.log_directory);
                ui.end_row();

                ui.label("日志文件名");
                ui.text_edit_singleline(&mut self.settings.form.log_file_name);
                ui.end_row();

                ui.label("导出调试产物");
                ui.checkbox(&mut self.settings.form.export_artifacts, "启用");
                ui.end_row();

                ui.label("调试导出目录");
                ui.text_edit_singleline(&mut self.settings.form.export_directory);
                ui.end_row();
            });

        ui.separator();
        ui.label(format!(
            "解析后的日志路径：{}",
            self.settings
                .form
                .resolved_log_path(&self.working_directory)
                .display()
        ));
        ui.label(format!(
            "解析后的调试导出目录：{}",
            self.settings
                .form
                .resolved_debug_directory(&self.working_directory)
                .display()
        ));
        if self.settings.is_dirty() {
            ui.colored_label(egui::Color32::YELLOW, "当前有未保存的修改。");
        } else {
            ui.label("当前没有未保存的修改。");
        }
    }
}

impl eframe::App for ArkAgentDesktopApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync.poll_running_task();
        self.device.poll_running_task(context);
        self.vision.poll_running_task(context);
        self.skland_login
            .poll_running_task(context, &self.working_directory);
        if self.sync.is_running()
            || self.device.is_running()
            || self.vision.is_running()
            || self.skland_login.is_running()
        {
            context.request_repaint_after(Duration::from_millis(200));
        }

        egui::TopBottomPanel::top("top_bar").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("方舟看号台");
                ui.label("本地同步、设备接入与配置壳层");
            });
        });

        egui::SidePanel::left("navigation")
            .resizable(false)
            .default_width(180.0)
            .show(context, |ui| {
                ui.heading("导航");
                ui.separator();
                ui.selectable_value(&mut self.current_page, Page::Dashboard, "仪表盘");
                ui.selectable_value(&mut self.current_page, Page::Device, "设备");
                ui.selectable_value(&mut self.current_page, Page::Vision, "视觉调试");
                ui.selectable_value(&mut self.current_page, Page::SklandLogin, "森空岛登录");
                ui.selectable_value(&mut self.current_page, Page::Sync, "同步");
                ui.selectable_value(&mut self.current_page, Page::Settings, "设置");
            });

        egui::CentralPanel::default().show(context, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut has_notice = false;

                    for notice in &self.startup_notices {
                        render_notice(ui, notice);
                        has_notice = true;
                    }

                    if let Some(notice) = self.sync.notice.as_ref() {
                        render_notice(ui, notice);
                        has_notice = true;
                    }

                    if let Some(notice) = self.device.notice.as_ref() {
                        render_notice(ui, notice);
                        has_notice = true;
                    }

                    if let Some(notice) = self.vision.notice.as_ref() {
                        render_notice(ui, notice);
                        has_notice = true;
                    }

                    if let Some(notice) = self.settings.notice.as_ref() {
                        render_notice(ui, notice);
                        has_notice = true;
                    }

                    if let Some(notice) = self.skland_login.notice.as_ref() {
                        render_notice(ui, notice);
                        has_notice = true;
                    }

                    if has_notice {
                        ui.separator();
                    }

                    match self.current_page {
                        Page::Dashboard => self.render_dashboard(ui),
                        Page::Device => self.render_device(ui),
                        Page::Vision => self.render_vision(ui),
                        Page::SklandLogin => self.render_skland_login(ui),
                        Page::Sync => self.render_sync(ui),
                        Page::Settings => self.render_settings(ui),
                    }
                });
        });

        self.skland_login.render_qr_popup(context);
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Page {
    Dashboard,
    Device,
    Vision,
    SklandLogin,
    Sync,
    Settings,
}

struct DevicePageState {
    preferred_serial_text: String,
    discovery_instance_count_text: String,
    tap_x_text: String,
    tap_y_text: String,
    swipe_x1_text: String,
    swipe_y1_text: String,
    swipe_x2_text: String,
    swipe_y2_text: String,
    swipe_duration_ms_text: String,
    keyevent_code_text: String,
    connection: Option<DeviceConnectionInfo>,
    preview_texture: Option<egui::TextureHandle>,
    preview_dimensions: Option<[usize; 2]>,
    preview_bytes_len: Option<usize>,
    preview_loaded_at: Option<String>,
    notice: Option<UiNotice>,
    running_task: Option<RunningDeviceTask>,
}

impl DevicePageState {
    fn new() -> Self {
        Self {
            preferred_serial_text: String::new(),
            discovery_instance_count_text: DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT.to_string(),
            tap_x_text: "960".to_string(),
            tap_y_text: "540".to_string(),
            swipe_x1_text: "400".to_string(),
            swipe_y1_text: "540".to_string(),
            swipe_x2_text: "1520".to_string(),
            swipe_y2_text: "540".to_string(),
            swipe_duration_ms_text: "300".to_string(),
            keyevent_code_text: "4".to_string(),
            connection: None,
            preview_texture: None,
            preview_dimensions: None,
            preview_bytes_len: None,
            preview_loaded_at: None,
            notice: None,
            running_task: None,
        }
    }

    fn is_running(&self) -> bool {
        self.running_task.is_some()
    }

    fn running_label(&self) -> Option<&'static str> {
        self.running_task.as_ref().map(|task| task.kind.label())
    }

    fn status_label(&self) -> String {
        if let Some(task) = self.running_task.as_ref() {
            return task.kind.label().to_string();
        }

        self.connection
            .as_ref()
            .map(|connection| format!("已连接 {}", connection.serial))
            .unwrap_or_else(|| "未连接".to_string())
    }

    fn preferred_serial(&self) -> Option<String> {
        let trimmed = self.preferred_serial_text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn clear_preview(&mut self) {
        self.preview_texture = None;
        self.preview_dimensions = None;
        self.preview_bytes_len = None;
        self.preview_loaded_at = None;
    }

    fn start_connect(&mut self, adb_executable: Option<PathBuf>) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有设备任务正在后台执行，请等待完成"));
            return;
        }

        let request = match self.build_connect_request(adb_executable) {
            Ok(request) => request,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };

        self.notice = Some(UiNotice::info("设备连接检查已开始，正在后台执行"));
        self.running_task = Some(RunningDeviceTask {
            kind: DeviceTaskKind::Connect,
            handle: thread::spawn(move || {
                DeviceTaskFinished::Connect(run_device_connect_task(request))
            }),
        });
    }

    fn start_capture(&mut self, adb_executable: Option<PathBuf>) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有设备任务正在后台执行，请等待完成"));
            return;
        }

        let request = match self.build_capture_request(adb_executable) {
            Ok(request) => request,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };

        self.notice = Some(UiNotice::info("设备截图任务已开始，正在后台执行"));
        self.running_task = Some(RunningDeviceTask {
            kind: DeviceTaskKind::Capture,
            handle: thread::spawn(move || {
                DeviceTaskFinished::Capture(run_device_capture_task(request))
            }),
        });
    }

    fn start_input_action(&mut self, adb_executable: Option<PathBuf>, action: DeviceInputAction) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有设备任务正在后台执行，请等待完成"));
            return;
        }

        let action_label = action.label_zh();
        let request = match self.build_input_request(adb_executable, action) {
            Ok(request) => request,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };

        self.notice = Some(UiNotice::info(format!(
            "设备{action_label}任务已开始，正在后台执行"
        )));
        self.running_task = Some(RunningDeviceTask {
            kind: DeviceTaskKind::Input(request.action.clone()),
            handle: thread::spawn(move || {
                DeviceTaskFinished::Input(run_device_input_task(request))
            }),
        });
    }

    fn poll_running_task(&mut self, context: &egui::Context) {
        if !self
            .running_task
            .as_ref()
            .is_some_and(|task| task.handle.is_finished())
        {
            return;
        }

        let task = self
            .running_task
            .take()
            .expect("finished device task should exist");

        match task.handle.join() {
            Ok(DeviceTaskFinished::Connect(result)) => match result {
                Ok(connection) => {
                    tracing::info!(
                        adb = %connection.adb_executable.display(),
                        serial = %connection.serial,
                        source = %connection.selection_source.label_zh(),
                        "desktop device connect completed"
                    );
                    self.connection = Some(connection.clone());
                    self.notice = Some(UiNotice::success(format!(
                        "设备已连接：{}（{}，adb：{}）",
                        connection.serial,
                        connection.selection_source.label_zh(),
                        connection.adb_executable.display()
                    )));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop device connect failed");
                    self.notice = Some(UiNotice::error(format!("设备连接失败：{error}")));
                }
            },
            Ok(DeviceTaskFinished::Capture(result)) => match result {
                Ok(capture) => {
                    tracing::info!(
                        adb = %capture.connection.adb_executable.display(),
                        serial = %capture.connection.serial,
                        bytes = capture.png_bytes.len(),
                        "desktop device screenshot capture completed"
                    );

                    match load_png_texture(context, capture.png_bytes.as_slice()) {
                        Ok((texture, dimensions)) => {
                            self.connection = Some(capture.connection.clone());
                            self.preview_texture = Some(texture);
                            self.preview_dimensions = Some(dimensions);
                            self.preview_bytes_len = Some(capture.png_bytes.len());
                            self.preview_loaded_at = Some(
                                OffsetDateTime::now_utc()
                                    .format(&Rfc3339)
                                    .unwrap_or_else(|_| "<未知>".to_string()),
                            );
                            self.notice = Some(UiNotice::success(format!(
                                "已抓取设备截图：{}（{} x {}）",
                                capture.connection.serial, dimensions[0], dimensions[1]
                            )));
                        }
                        Err(error) => {
                            tracing::error!(%error, "desktop device screenshot decode failed");
                            self.notice = Some(UiNotice::error(format!(
                                "截图已抓取，但预览解码失败：{error}"
                            )));
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "desktop device screenshot capture failed");
                    self.notice = Some(UiNotice::error(format!("设备截图失败：{error}")));
                }
            },
            Ok(DeviceTaskFinished::Input(result)) => match result {
                Ok(input) => {
                    tracing::info!(
                        adb = %input.connection.adb_executable.display(),
                        serial = %input.connection.serial,
                        action = input.action.label_zh(),
                        "desktop device input completed"
                    );
                    self.connection = Some(input.connection.clone());
                    self.notice = Some(UiNotice::success(format!(
                        "设备{}已发送到 {}",
                        input.action.label_zh(),
                        input.connection.serial
                    )));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop device input failed");
                    self.notice = Some(UiNotice::error(format!("设备输入失败：{error}")));
                }
            },
            Err(_) => {
                self.notice = Some(UiNotice::error(format!(
                    "{} 任务线程异常退出",
                    task.kind.display_name()
                )));
            }
        }
    }

    fn build_connect_request(
        &self,
        adb_executable: Option<PathBuf>,
    ) -> Result<DeviceConnectRequest, String> {
        Ok(DeviceConnectRequest {
            adb_executable,
            preferred_serial: self.preferred_serial(),
            discovery_instance_count: self.parse_discovery_instance_count()?,
        })
    }

    fn build_capture_request(
        &self,
        adb_executable: Option<PathBuf>,
    ) -> Result<ScreenshotCaptureRequest, String> {
        Ok(ScreenshotCaptureRequest {
            adb_executable,
            preferred_serial: self.preferred_serial(),
            discovery_instance_count: self.parse_discovery_instance_count()?,
        })
    }

    fn build_input_request(
        &self,
        adb_executable: Option<PathBuf>,
        action: DeviceInputAction,
    ) -> Result<DeviceInputRequest, String> {
        Ok(DeviceInputRequest {
            adb_executable,
            preferred_serial: self.preferred_serial(),
            discovery_instance_count: self.parse_discovery_instance_count()?,
            action,
        })
    }

    fn parse_discovery_instance_count(&self) -> Result<u16, String> {
        let trimmed = self.discovery_instance_count_text.trim();
        if trimmed.is_empty() {
            return Ok(DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT);
        }

        let value = trimmed
            .parse::<u16>()
            .map_err(|_| "自动探测实例数必须是 1 到 128 之间的整数".to_string())?;
        if value == 0 || value > 128 {
            return Err("自动探测实例数必须是 1 到 128 之间的整数".to_string());
        }

        Ok(value)
    }

    fn tap_action(&self) -> Result<DeviceInputAction, String> {
        Ok(DeviceInputAction::Tap {
            x: self.parse_u32_field(&self.tap_x_text, "点按 X")?,
            y: self.parse_u32_field(&self.tap_y_text, "点按 Y")?,
        })
    }

    fn swipe_action(&self) -> Result<DeviceInputAction, String> {
        Ok(DeviceInputAction::Swipe {
            x1: self.parse_u32_field(&self.swipe_x1_text, "滑动起点 X")?,
            y1: self.parse_u32_field(&self.swipe_y1_text, "滑动起点 Y")?,
            x2: self.parse_u32_field(&self.swipe_x2_text, "滑动终点 X")?,
            y2: self.parse_u32_field(&self.swipe_y2_text, "滑动终点 Y")?,
            duration_ms: self.parse_u32_field(&self.swipe_duration_ms_text, "滑动时长毫秒")?,
        })
    }

    fn keyevent_action(&self) -> Result<DeviceInputAction, String> {
        let value = self
            .keyevent_code_text
            .trim()
            .parse::<u16>()
            .map_err(|_| "按键码必须是 0 到 65535 之间的整数".to_string())?;
        Ok(DeviceInputAction::KeyEvent { key_code: value })
    }

    fn parse_u32_field(&self, value: &str, field_name: &str) -> Result<u32, String> {
        value
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("{field_name} 必须是非负整数"))
    }
}

struct RunningDeviceTask {
    kind: DeviceTaskKind,
    handle: JoinHandle<DeviceTaskFinished>,
}

#[derive(Clone)]
enum DeviceTaskKind {
    Connect,
    Capture,
    Input(DeviceInputAction),
}

impl DeviceTaskKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Connect => "后台任务：设备连接检查中",
            Self::Capture => "后台任务：设备截图抓取中",
            Self::Input(DeviceInputAction::Tap { .. }) => "后台任务：设备点按中",
            Self::Input(DeviceInputAction::Swipe { .. }) => "后台任务：设备滑动中",
            Self::Input(DeviceInputAction::KeyEvent { .. }) => "后台任务：设备按键中",
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::Connect => "设备连接检查",
            Self::Capture => "设备截图抓取",
            Self::Input(DeviceInputAction::Tap { .. }) => "设备点按",
            Self::Input(DeviceInputAction::Swipe { .. }) => "设备滑动",
            Self::Input(DeviceInputAction::KeyEvent { .. }) => "设备按键",
        }
    }
}

enum DeviceTaskFinished {
    Connect(Result<DeviceConnectionInfo, String>),
    Capture(Result<ScreenshotCaptureResult, String>),
    Input(Result<DeviceInputResult, String>),
}

fn run_device_connect_task(request: DeviceConnectRequest) -> Result<DeviceConnectionInfo, String> {
    open_device_session(&request)
        .map(|result| result.connection)
        .map_err(|error| error.to_string())
}

fn run_device_capture_task(
    request: ScreenshotCaptureRequest,
) -> Result<ScreenshotCaptureResult, String> {
    capture_device_screenshot(&request).map_err(|error| error.to_string())
}

fn run_device_input_task(request: DeviceInputRequest) -> Result<DeviceInputResult, String> {
    send_device_input(&request).map_err(|error| error.to_string())
}

fn render_adb_devices_summary(devices: &[akbox_device::AdbDeviceEntry]) -> String {
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

fn load_png_texture(
    context: &egui::Context,
    png_bytes: &[u8],
) -> Result<(egui::TextureHandle, [usize; 2]), String> {
    load_png_texture_with_name(context, "device-preview-texture", png_bytes)
}

fn load_png_texture_with_name(
    context: &egui::Context,
    texture_name: &str,
    png_bytes: &[u8],
) -> Result<(egui::TextureHandle, [usize; 2]), String> {
    let decoded = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    let texture = context.load_texture(texture_name, color_image, egui::TextureOptions::LINEAR);

    Ok((texture, size))
}

fn render_skland_sample_operator_summary(
    sample: Option<&akbox_data::SklandOperatorSample>,
) -> String {
    match sample {
        Some(sample) => format!(
            "{} / {} / 精英{} Lv{} / 技能{} / 专精[{}/{}/{}] / 模组 {} {}",
            sample.name_zh,
            sample.operator_id,
            sample.elite_stage,
            sample.level,
            sample.skill_level,
            sample.mastery_1,
            sample.mastery_2,
            sample.mastery_3,
            sample.module_state.as_deref().unwrap_or("<无>"),
            sample
                .module_level
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        None => "<无>".to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisionInputSourceMode {
    CurrentDeviceScreenshot,
    LocalPng,
}

#[derive(Clone)]
struct VisionPagePreset {
    key: String,
    page_id: String,
    display_name: String,
    config_path: PathBuf,
    templates_root: PathBuf,
    marker_count: usize,
    roi_count: usize,
}

impl VisionPagePreset {
    fn user_label(&self) -> String {
        format!("{} ({})", self.display_name, self.page_id)
    }
}

struct VisionDebugPageState {
    available_pages: Vec<VisionPagePreset>,
    selected_page_key: Option<String>,
    input_source_mode: VisionInputSourceMode,
    page_config_path_text: String,
    page_id_text: String,
    input_png_path_text: String,
    templates_root_path_text: String,
    output_dir_path_text: String,
    source_texture: Option<egui::TextureHandle>,
    source_dimensions: Option<[usize; 2]>,
    source_bytes_len: Option<usize>,
    last_result: Option<VisionInspectGuiResult>,
    notice: Option<UiNotice>,
    running_task: Option<RunningVisionTask>,
}

impl VisionDebugPageState {
    fn new(working_directory: &Path) -> Self {
        let mut state = Self {
            available_pages: Vec::new(),
            selected_page_key: None,
            input_source_mode: VisionInputSourceMode::CurrentDeviceScreenshot,
            page_config_path_text: String::new(),
            page_id_text: String::new(),
            input_png_path_text: String::new(),
            templates_root_path_text: String::new(),
            output_dir_path_text: String::new(),
            source_texture: None,
            source_dimensions: None,
            source_bytes_len: None,
            last_result: None,
            notice: None,
            running_task: None,
        };

        match state.refresh_page_catalog(working_directory) {
            Ok(()) => {
                if state.available_pages.is_empty() {
                    state.notice = Some(UiNotice::warning(
                        "当前未在 assets/templates/pages 下发现页面模板，后续放入模板文件后这里会自动列出。",
                    ));
                }
            }
            Err(error) => {
                state.notice = Some(UiNotice::warning(format!("初始化视觉调试页失败：{error}")));
            }
        }

        state
    }

    fn is_running(&self) -> bool {
        self.running_task.is_some()
    }

    fn running_label(&self) -> Option<&'static str> {
        self.running_task.as_ref().map(|task| task.kind.label())
    }

    fn run_button_label(&self) -> &'static str {
        match self.input_source_mode {
            VisionInputSourceMode::CurrentDeviceScreenshot => "使用当前设备截图并运行",
            VisionInputSourceMode::LocalPng => "使用本地 PNG 运行",
        }
    }

    fn selected_page(&self) -> Option<&VisionPagePreset> {
        let selected_key = self.selected_page_key.as_deref()?;
        self.available_pages
            .iter()
            .find(|page| page.key == selected_key)
    }

    fn selected_page_label(&self) -> String {
        self.selected_page()
            .map(VisionPagePreset::user_label)
            .unwrap_or_else(|| "未发现页面模板".to_string())
    }

    fn set_selected_page_key(&mut self, selected_page_key: Option<String>) {
        self.selected_page_key = selected_page_key;
        self.apply_selected_page_defaults();
    }

    fn apply_selected_page_defaults(&mut self) {
        let selected = self
            .selected_page()
            .map(|page| (page.config_path.clone(), page.page_id.clone()));
        if let Some((config_path, page_id)) = selected {
            self.page_config_path_text = config_path.display().to_string();
            self.page_id_text = page_id;
        }
    }

    fn refresh_page_catalog(&mut self, working_directory: &Path) -> Result<(), String> {
        let templates_root = discover_default_vision_templates_root(working_directory);
        let available_pages = discover_vision_page_presets(&templates_root)?;
        let previous_key = self.selected_page_key.clone();

        self.available_pages = available_pages;
        self.selected_page_key = previous_key
            .filter(|key| self.available_pages.iter().any(|page| page.key == *key))
            .or_else(|| self.available_pages.first().map(|page| page.key.clone()));

        if self.available_pages.is_empty() {
            self.page_config_path_text.clear();
            self.page_id_text.clear();
            return Ok(());
        }

        self.apply_selected_page_defaults();
        Ok(())
    }

    fn clear_result(&mut self) {
        self.source_texture = None;
        self.source_dimensions = None;
        self.source_bytes_len = None;
        self.last_result = None;
        self.notice = Some(UiNotice::info("已清空视觉调试结果"));
    }

    fn resolved_templates_root(&self, working_directory: &Path) -> PathBuf {
        non_empty_path(&self.templates_root_path_text)
            .or_else(|| self.selected_page().map(|page| page.templates_root.clone()))
            .or_else(|| {
                non_empty_path(&self.page_config_path_text)
                    .map(|path| default_templates_root_for_config(path.as_path()))
            })
            .unwrap_or_else(|| discover_default_vision_templates_root(working_directory))
    }

    fn resolved_output_dir(&self, working_directory: &Path) -> PathBuf {
        non_empty_path(&self.output_dir_path_text)
            .unwrap_or_else(|| default_vision_output_dir(working_directory, &self.page_id_text))
    }

    fn build_request(
        &self,
        working_directory: &Path,
        capture_request: Option<ScreenshotCaptureRequest>,
    ) -> Result<VisionInspectRequest, String> {
        let page_config_path = non_empty_path(&self.page_config_path_text).ok_or_else(|| {
            "当前还没有可用页面配置，请先刷新页面模板或在高级选项中手工填写".to_string()
        })?;
        let page_id = self.page_id_text.trim();
        if page_id.is_empty() {
            return Err("页面 ID 不能为空".to_string());
        }

        let source = match self.input_source_mode {
            VisionInputSourceMode::LocalPng => VisionInspectSourceRequest::LocalPng(
                non_empty_path(&self.input_png_path_text)
                    .ok_or_else(|| "本地 PNG 路径不能为空".to_string())?,
            ),
            VisionInputSourceMode::CurrentDeviceScreenshot => {
                VisionInspectSourceRequest::CurrentDeviceScreenshot(
                    capture_request
                        .ok_or_else(|| "当前设备截图模式需要有效的设备连接参数".to_string())?,
                )
            }
        };

        Ok(VisionInspectRequest {
            page_config_path,
            page_id: page_id.to_string(),
            templates_root: self.resolved_templates_root(working_directory),
            output_dir: self.resolved_output_dir(working_directory),
            source,
        })
    }

    fn start_inspect(
        &mut self,
        working_directory: &Path,
        capture_request: Option<ScreenshotCaptureRequest>,
    ) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning(
                "已有视觉调试任务正在后台执行，请等待完成",
            ));
            return;
        }

        let request = match self.build_request(working_directory, capture_request) {
            Ok(request) => request,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };

        self.notice = Some(UiNotice::info(match self.input_source_mode {
            VisionInputSourceMode::CurrentDeviceScreenshot => {
                "当前设备截图调试已开始，正在后台抓图并识别"
            }
            VisionInputSourceMode::LocalPng => "本地 PNG 调试已开始，正在后台识别",
        }));
        self.running_task = Some(RunningVisionTask {
            kind: VisionTaskKind::Inspect,
            handle: thread::spawn(move || {
                VisionTaskFinished::Inspect(run_vision_inspect_task(request))
            }),
        });
    }

    fn poll_running_task(&mut self, context: &egui::Context) {
        if !self
            .running_task
            .as_ref()
            .is_some_and(|task| task.handle.is_finished())
        {
            return;
        }

        let task = self
            .running_task
            .take()
            .expect("finished vision task should exist");

        match task.handle.join() {
            Ok(VisionTaskFinished::Inspect(result)) => match result {
                Ok(task_result) => {
                    tracing::info!(
                        page_id = %task_result.result.page_id,
                        matched = task_result.result.page_confirmation.matched,
                        roi_count = task_result.result.roi_outputs.len(),
                        manifest = %task_result.result.manifest_path.display(),
                        source = %task_result.result.source_label,
                        "desktop vision inspect completed"
                    );

                    self.source_bytes_len = Some(task_result.input_png_bytes.len());
                    match load_png_texture_with_name(
                        context,
                        "vision-source-preview-texture",
                        task_result.input_png_bytes.as_slice(),
                    ) {
                        Ok((texture, dimensions)) => {
                            self.source_texture = Some(texture);
                            self.source_dimensions = Some(dimensions);
                            self.notice = Some(UiNotice::success(format!(
                                "视觉调试完成：{}；页面{}；ROI {} 个",
                                task_result.result.source_label,
                                if task_result.result.page_confirmation.matched {
                                    "匹配成功"
                                } else {
                                    "未稳定匹配"
                                },
                                task_result.result.roi_outputs.len()
                            )));
                        }
                        Err(error) => {
                            tracing::error!(%error, "desktop vision preview decode failed");
                            self.source_texture = None;
                            self.source_dimensions = None;
                            self.notice = Some(UiNotice::warning(format!(
                                "视觉调试已完成，但源图预览解码失败：{error}"
                            )));
                        }
                    }

                    self.last_result = Some(task_result.result);
                }
                Err(error) => {
                    tracing::error!(%error, "desktop vision inspect failed");
                    self.notice = Some(UiNotice::error(format!("视觉调试失败：{error}")));
                }
            },
            Err(_) => {
                self.notice = Some(UiNotice::error("视觉调试任务线程异常退出"));
            }
        }
    }
}

#[derive(Debug)]
enum VisionInspectSourceRequest {
    LocalPng(PathBuf),
    CurrentDeviceScreenshot(ScreenshotCaptureRequest),
}

#[derive(Debug)]
struct VisionInspectRequest {
    page_config_path: PathBuf,
    page_id: String,
    templates_root: PathBuf,
    output_dir: PathBuf,
    source: VisionInspectSourceRequest,
}

struct VisionInspectTaskResult {
    result: VisionInspectGuiResult,
    input_png_bytes: Vec<u8>,
}

struct VisionInspectGuiResult {
    page_config_path: PathBuf,
    page_id: String,
    page_display_name: String,
    input_png_path: PathBuf,
    source_label: String,
    output_dir: PathBuf,
    manifest_path: PathBuf,
    page_confirmation: PageConfirmationResult,
    roi_outputs: Vec<VisionRoiOutput>,
}

struct VisionRoiOutput {
    roi_id: String,
    display_name: String,
    purpose: RoiPurpose,
    output_png_path: PathBuf,
    ocr_status: String,
    ocr_message: String,
}

struct RunningVisionTask {
    kind: VisionTaskKind,
    handle: JoinHandle<VisionTaskFinished>,
}

enum VisionTaskKind {
    Inspect,
}

impl VisionTaskKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Inspect => "后台任务：视觉调试运行中",
        }
    }
}

enum VisionTaskFinished {
    Inspect(Result<VisionInspectTaskResult, String>),
}

fn run_vision_inspect_task(
    request: VisionInspectRequest,
) -> Result<VisionInspectTaskResult, String> {
    fs::create_dir_all(&request.output_dir).map_err(|error| {
        format!(
            "创建视觉调试输出目录 {} 失败：{error}",
            request.output_dir.display()
        )
    })?;

    let (screenshot, input_png_path, source_label) = match request.source {
        VisionInspectSourceRequest::LocalPng(input_png_path) => {
            let screenshot = fs::read(&input_png_path).map_err(|error| {
                format!("读取输入 PNG {} 失败：{error}", input_png_path.display())
            })?;
            let source_label = format!("本地 PNG：{}", input_png_path.display());
            (screenshot, input_png_path, source_label)
        }
        VisionInspectSourceRequest::CurrentDeviceScreenshot(capture_request) => {
            let capture = capture_device_screenshot(&capture_request)
                .map_err(|error| format!("抓取当前设备截图失败：{error}"))?;
            let input_png_path = request.output_dir.join("device-screenshot.png");
            fs::write(&input_png_path, &capture.png_bytes).map_err(|error| {
                format!(
                    "写入当前设备截图 {} 失败：{error}",
                    input_png_path.display()
                )
            })?;
            let source_label = format!("当前设备截图：{}", capture.connection.serial);
            (capture.png_bytes, input_png_path, source_label)
        }
    };

    let catalog = load_page_state_catalog_from_path(&request.page_config_path)
        .map_err(|error| error.to_string())?;
    let page = catalog
        .find_page(&request.page_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "页面配置 {} 中找不到 page_id `{}`",
                request.page_config_path.display(),
                request.page_id
            )
        })?;

    let confirmation = evaluate_page_confirmation_from_png(
        &page,
        screenshot.as_slice(),
        request.templates_root.as_path(),
    )
    .map_err(|error| error.to_string())?;
    let crops =
        crop_all_rois_from_png(&page, screenshot.as_slice()).map_err(|error| error.to_string())?;

    let mut roi_manifest = Vec::new();
    let mut roi_outputs = Vec::new();
    for crop in &crops {
        let file_name = format!("{}.png", sanitize_debug_file_name(&crop.roi.roi_id));
        let output_path = request.output_dir.join(&file_name);
        fs::write(&output_path, &crop.png_bytes)
            .map_err(|error| format!("写入 ROI PNG {} 失败：{error}", output_path.display()))?;

        let (ocr_manifest, ocr_status, ocr_message) = match crop.roi.purpose {
            RoiPurpose::NumericOcr | RoiPurpose::ShortTextOcr => {
                let request = OcrRequest {
                    numeric_only: matches!(crop.roi.purpose, RoiPurpose::NumericOcr),
                    ..OcrRequest::default()
                };
                match recognize_text_from_png(&crop.png_bytes, &request) {
                    Ok(result) => (
                        json!({
                            "status": "ok",
                            "backend": result.backend,
                            "text": result.text,
                            "lines": result.lines,
                        }),
                        "ok".to_string(),
                        if result.text.trim().is_empty() {
                            "OCR 已运行，但当前没有识别到文本".to_string()
                        } else {
                            result.text
                        },
                    ),
                    Err(error) => (
                        json!({
                            "status": "error",
                            "message": error.to_string(),
                        }),
                        "error".to_string(),
                        error.to_string(),
                    ),
                }
            }
            _ => (
                json!({
                    "status": "skipped",
                    "message": "当前 ROI purpose 不走 OCR"
                }),
                "skipped".to_string(),
                "当前 ROI purpose 不走 OCR".to_string(),
            ),
        };

        roi_manifest.push(json!({
            "roi_id": crop.roi.roi_id,
            "display_name": crop.roi.display_name,
            "purpose": crop.roi.purpose,
            "output_png": output_path.display().to_string(),
            "artifact_payload": crop.artifact_payload(),
            "ocr": ocr_manifest,
        }));
        roi_outputs.push(VisionRoiOutput {
            roi_id: crop.roi.roi_id.clone(),
            display_name: crop.roi.display_name.clone(),
            purpose: crop.roi.purpose,
            output_png_path: output_path,
            ocr_status,
            ocr_message,
        });
    }

    let manifest = json!({
        "page_config_path": request.page_config_path.display().to_string(),
        "page_id": page.page_id.clone(),
        "page_display_name": page.display_name.clone(),
        "templates_root": request.templates_root.display().to_string(),
        "input_png": input_png_path.display().to_string(),
        "source_label": source_label,
        "page_confirmation": confirmation.clone(),
        "roi_outputs": roi_manifest,
    });
    let manifest_path = request.output_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("序列化视觉调试 manifest 失败：{error}"))?;
    fs::write(&manifest_path, manifest_json).map_err(|error| {
        format!(
            "写入视觉调试 manifest {} 失败：{error}",
            manifest_path.display()
        )
    })?;

    Ok(VisionInspectTaskResult {
        result: VisionInspectGuiResult {
            page_config_path: request.page_config_path,
            page_id: page.page_id,
            page_display_name: page.display_name,
            input_png_path,
            source_label,
            output_dir: request.output_dir,
            manifest_path,
            page_confirmation: confirmation,
            roi_outputs,
        },
        input_png_bytes: screenshot,
    })
}

struct SklandLoginPageState {
    auth_file_path_text: String,
    database_path_text: String,
    uid_file_text: String,
    status_text: String,
    local_auth_summary: Option<SklandAuthFileSummary>,
    last_player_info_inspect: Option<SklandPlayerInfoInspectOutcome>,
    last_status_building_import: Option<SklandStatusBuildingImportOutcome>,
    last_operator_import: Option<SklandOperatorImportOutcome>,
    qr_texture: Option<egui::TextureHandle>,
    qr_dimensions: Option<[usize; 2]>,
    show_qr_popup: bool,
    notice: Option<UiNotice>,
    running_task: Option<RunningSklandLoginTask>,
    running_profile_task: Option<RunningSklandProfileTask>,
}

impl SklandLoginPageState {
    fn new(working_directory: &Path) -> Self {
        let mut state = Self {
            auth_file_path_text: default_skland_auth_file_path(working_directory)
                .display()
                .to_string(),
            database_path_text: default_database_path(working_directory)
                .display()
                .to_string(),
            uid_file_text: default_skland_uid_file_value(),
            status_text: "尚未开始扫码登录".to_string(),
            local_auth_summary: None,
            last_player_info_inspect: None,
            last_status_building_import: None,
            last_operator_import: None,
            qr_texture: None,
            qr_dimensions: None,
            show_qr_popup: false,
            notice: None,
            running_task: None,
            running_profile_task: None,
        };

        if let Err(error) = state.refresh_local_auth_status(working_directory) {
            state.notice = Some(UiNotice::warning(format!(
                "初始化森空岛本地鉴权状态失败：{error}"
            )));
        }

        state
    }

    fn is_running(&self) -> bool {
        self.running_task.is_some() || self.running_profile_task.is_some()
    }

    fn running_label(&self) -> Option<&'static str> {
        self.running_task
            .as_ref()
            .map(|task| task.kind.label())
            .or_else(|| {
                self.running_profile_task
                    .as_ref()
                    .map(|task| task.kind.label())
            })
    }

    fn status_label(&self) -> &str {
        let trimmed = self.status_text.trim();
        if trimmed.is_empty() {
            "尚未开始扫码登录"
        } else {
            trimmed
        }
    }

    fn local_auth_status_label(&self) -> &'static str {
        match self.local_auth_summary.as_ref() {
            Some(summary) if summary.has_access_token && summary.has_cred && summary.has_token => {
                "已写入"
            }
            Some(summary)
                if summary.exists
                    && (summary.has_access_token
                        || summary.has_cred
                        || summary.has_token
                        || summary.has_user_id) =>
            {
                "部分已写入"
            }
            Some(summary) if summary.exists => "文件已存在但尚未写入凭据",
            Some(_) => "尚未创建",
            None => "未初始化",
        }
    }

    fn can_reopen_qr_popup(&self) -> bool {
        self.running_task.is_some() && self.qr_texture.is_some()
    }

    fn resolved_auth_file_path(&self, working_directory: &Path) -> PathBuf {
        let trimmed = self.auth_file_path_text.trim();
        let path = if trimmed.is_empty() {
            default_skland_auth_file_path(working_directory)
        } else {
            PathBuf::from(trimmed)
        };

        if path.is_relative() {
            working_directory.join(path)
        } else {
            path
        }
    }

    fn uid_file_value(&self) -> String {
        let trimmed = self.uid_file_text.trim();
        if trimmed.is_empty() {
            default_skland_uid_file_value()
        } else {
            trimmed.to_string()
        }
    }

    fn resolved_database_path(&self, working_directory: &Path) -> PathBuf {
        let trimmed = self.database_path_text.trim();
        let path = if trimmed.is_empty() {
            default_database_path(working_directory)
        } else {
            PathBuf::from(trimmed)
        };

        if path.is_relative() {
            working_directory.join(path)
        } else {
            path
        }
    }

    fn refresh_local_auth_status(&mut self, working_directory: &Path) -> Result<String, String> {
        let auth_file_path = self.resolved_auth_file_path(working_directory);
        let loaded = read_skland_auth_file_or_default(&auth_file_path)?;
        if loaded.exists {
            self.uid_file_text = loaded.file.skland.uid_file.clone();
        } else if self.uid_file_text.trim().is_empty() {
            self.uid_file_text = default_skland_uid_file_value();
        }
        self.local_auth_summary = Some(build_skland_auth_file_summary(&auth_file_path, &loaded));

        Ok(if loaded.exists {
            format!("已刷新本地鉴权文件状态：{}", auth_file_path.display())
        } else {
            format!(
                "本地鉴权文件尚未创建：{}；首次扫码成功后会自动生成",
                auth_file_path.display()
            )
        })
    }

    fn start_login(&mut self, working_directory: &Path) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning(
                "已有森空岛扫码登录任务正在后台执行，请等待完成",
            ));
            return;
        }

        let auth_file_path = self.resolved_auth_file_path(working_directory);
        let request = SklandLoginTaskRequest {
            auth_file_path,
            uid_file_value: self.uid_file_value(),
        };
        let (sender, receiver) = mpsc::channel();

        self.clear_qr_state();
        self.show_qr_popup = true;
        self.status_text = "正在申请森空岛扫码二维码".to_string();
        self.notice = Some(UiNotice::info(
            "森空岛扫码登录已开始，二维码生成后会自动弹出",
        ));
        self.running_task = Some(RunningSklandLoginTask {
            kind: SklandLoginTaskKind::QrLogin,
            receiver,
            handle: thread::spawn(move || run_skland_qr_login_task(request, sender)),
        });
    }

    fn start_player_info_inspect(&mut self, working_directory: &Path) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有森空岛任务正在后台执行，请等待完成"));
            return;
        }

        let auth_file_path = self.resolved_auth_file_path(working_directory);
        let database_path = self.resolved_database_path(working_directory);
        self.status_text = "正在检查森空岛 player/info".to_string();
        self.notice = Some(UiNotice::info(
            "森空岛 player/info 检查已开始，正在后台执行",
        ));
        self.running_profile_task = Some(RunningSklandProfileTask {
            kind: SklandProfileTaskKind::InspectPlayerInfo,
            handle: thread::spawn(move || {
                SklandProfileTaskFinished::Inspect(run_skland_player_info_inspect_task(
                    auth_file_path,
                    database_path,
                ))
            }),
        });
    }

    fn start_status_building_import(&mut self, working_directory: &Path) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有森空岛任务正在后台执行，请等待完成"));
            return;
        }

        let auth_file_path = self.resolved_auth_file_path(working_directory);
        let database_path = self.resolved_database_path(working_directory);
        self.status_text = "正在导入森空岛账号/基建状态".to_string();
        self.notice = Some(UiNotice::info(
            "森空岛账号/基建状态导入已开始，正在后台执行",
        ));
        self.running_profile_task = Some(RunningSklandProfileTask {
            kind: SklandProfileTaskKind::ImportStatusBuilding,
            handle: thread::spawn(move || {
                SklandProfileTaskFinished::ImportStatusBuilding(
                    run_skland_status_building_import_task(auth_file_path, database_path),
                )
            }),
        });
    }

    fn start_operator_state_import(&mut self, working_directory: &Path) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有森空岛任务正在后台执行，请等待完成"));
            return;
        }

        let auth_file_path = self.resolved_auth_file_path(working_directory);
        let database_path = self.resolved_database_path(working_directory);
        self.status_text = "正在导入森空岛干员状态".to_string();
        self.notice = Some(UiNotice::info("森空岛干员状态导入已开始，正在后台执行"));
        self.running_profile_task = Some(RunningSklandProfileTask {
            kind: SklandProfileTaskKind::ImportOperatorState,
            handle: thread::spawn(move || {
                SklandProfileTaskFinished::Import(run_skland_operator_state_import_task(
                    auth_file_path,
                    database_path,
                ))
            }),
        });
    }

    fn poll_running_task(&mut self, context: &egui::Context, working_directory: &Path) {
        if let Some(task) = self.running_task.as_mut() {
            loop {
                match task.receiver.try_recv() {
                    Ok(SklandLoginProgress::Status(status)) => {
                        self.status_text = status;
                    }
                    Ok(SklandLoginProgress::QrReady(qr_payload)) => {
                        self.show_qr_popup = true;
                        self.status_text =
                            "二维码已生成，请使用森空岛 App 扫码并在手机端确认".to_string();
                        match load_skland_qr_texture(context, &qr_payload) {
                            Ok((texture, dimensions)) => {
                                self.qr_texture = Some(texture);
                                self.qr_dimensions = Some(dimensions);
                                self.notice =
                                    Some(UiNotice::info("二维码已生成，请在弹窗中扫码并确认登录"));
                            }
                            Err(error) => {
                                tracing::error!(%error, "failed to render skland qr code");
                                self.notice = Some(UiNotice::error(format!(
                                    "二维码已生成，但桌面预览渲染失败：{error}"
                                )));
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }

        if self
            .running_task
            .as_ref()
            .is_some_and(|task| task.handle.is_finished())
        {
            let task = self
                .running_task
                .take()
                .expect("finished skland task should exist");

            match task.handle.join() {
                Ok(Ok(success)) => {
                    tracing::info!(
                        auth_file_path = %success.auth_file_path.display(),
                        has_user_id = success.has_user_id,
                        "desktop skland qr login completed"
                    );
                    self.status_text = "登录成功，已写入本地鉴权文件".to_string();
                    self.notice = Some(UiNotice::success(format!(
                        "森空岛登录成功，已写入 {}",
                        success.auth_file_path.display()
                    )));
                    self.show_qr_popup = false;
                    self.clear_qr_state();
                    if let Err(error) = self.refresh_local_auth_status(working_directory) {
                        self.notice = Some(UiNotice::warning(format!(
                            "登录成功，但刷新本地鉴权文件状态失败：{error}"
                        )));
                    }
                }
                Ok(Err(error)) => {
                    tracing::error!(%error, "desktop skland qr login failed");
                    self.status_text = "扫码登录失败".to_string();
                    self.notice = Some(UiNotice::error(format!("森空岛扫码登录失败：{error}")));
                    self.clear_qr_state();
                }
                Err(_) => {
                    self.status_text = "扫码登录任务异常退出".to_string();
                    self.notice = Some(UiNotice::error("森空岛扫码登录任务线程异常退出"));
                    self.clear_qr_state();
                }
            }
        }

        if !self
            .running_profile_task
            .as_ref()
            .is_some_and(|task| task.handle.is_finished())
        {
            return;
        }

        let task = self
            .running_profile_task
            .take()
            .expect("finished skland profile task should exist");

        match task.handle.join() {
            Ok(SklandProfileTaskFinished::Inspect(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        revision = %outcome.revision,
                        char_count = outcome.char_count,
                        assist_count = outcome.assist_count,
                        "desktop skland player/info inspect completed"
                    );
                    self.last_operator_import = None;
                    self.last_player_info_inspect = Some(outcome);
                    self.status_text = "森空岛 player/info 检查完成".to_string();
                    self.notice =
                        Some(UiNotice::success("森空岛 player/info 检查完成，摘要已刷新"));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop skland player/info inspect failed");
                    self.status_text = "森空岛 player/info 检查失败".to_string();
                    self.notice = Some(UiNotice::error(format!(
                        "森空岛 player/info 检查失败：{error}"
                    )));
                }
            },
            Ok(SklandProfileTaskFinished::ImportStatusBuilding(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        revision = %outcome.inspect.revision,
                        player_status_snapshot_id = %outcome.player_status_snapshot_id,
                        base_building_snapshot_id = %outcome.base_building_snapshot_id,
                        "desktop skland status/building import completed"
                    );
                    self.last_player_info_inspect = Some(outcome.inspect.clone());
                    self.last_status_building_import = Some(outcome);
                    self.status_text = "森空岛账号/基建状态导入完成".to_string();
                    self.notice =
                        Some(UiNotice::success("森空岛账号/基建状态导入完成，摘要已刷新"));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop skland status/building import failed");
                    self.status_text = "森空岛账号/基建状态导入失败".to_string();
                    self.notice = Some(UiNotice::error(format!(
                        "森空岛账号/基建状态导入失败：{error}"
                    )));
                }
            },
            Ok(SklandProfileTaskFinished::Import(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        revision = %outcome.inspect.revision,
                        imported_row_count = outcome.imported_row_count,
                        snapshot_id = %outcome.snapshot_id,
                        "desktop skland operator import completed"
                    );
                    self.last_player_info_inspect = Some(outcome.inspect.clone());
                    self.last_operator_import = Some(outcome);
                    self.status_text = "森空岛干员状态导入完成".to_string();
                    self.notice = Some(UiNotice::success("森空岛干员状态导入完成，摘要已刷新"));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop skland operator import failed");
                    self.status_text = "森空岛干员状态导入失败".to_string();
                    self.notice = Some(UiNotice::error(format!("森空岛干员状态导入失败：{error}")));
                }
            },
            Err(_) => {
                self.status_text = "森空岛 player/info 任务异常退出".to_string();
                self.notice = Some(UiNotice::error("森空岛 player/info 任务线程异常退出"));
            }
        }
    }

    fn render_qr_popup(&mut self, context: &egui::Context) {
        if !self.show_qr_popup {
            return;
        }

        let mut open = self.show_qr_popup;
        egui::Window::new("森空岛扫码登录")
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("请使用森空岛 App 扫描下方二维码，并在手机端确认登录。");
                ui.label("登录成功后，桌面程序会把凭据写入本地忽略的 skland-auth.local.toml。");
                ui.separator();

                if let Some(texture) = self.qr_texture.as_ref() {
                    let image_size = texture.size_vec2();
                    ui.add(egui::Image::from_texture(texture).fit_to_exact_size(image_size));
                    if let Some(dimensions) = self.qr_dimensions {
                        ui.small(format!("二维码尺寸：{} x {}", dimensions[0], dimensions[1]));
                    }
                } else {
                    ui.add(egui::Spinner::new());
                    ui.label("正在生成二维码，请稍候。");
                }

                ui.separator();
                ui.label(format!("当前状态：{}", self.status_label()));
            });

        self.show_qr_popup = open;
    }

    fn clear_qr_state(&mut self) {
        self.qr_texture = None;
        self.qr_dimensions = None;
    }
}

struct RunningSklandLoginTask {
    kind: SklandLoginTaskKind,
    receiver: mpsc::Receiver<SklandLoginProgress>,
    handle: JoinHandle<Result<SklandLoginTaskSuccess, String>>,
}

enum SklandLoginTaskKind {
    QrLogin,
}

impl SklandLoginTaskKind {
    fn label(&self) -> &'static str {
        match self {
            Self::QrLogin => "后台任务：森空岛扫码登录进行中",
        }
    }
}

enum SklandLoginProgress {
    Status(String),
    QrReady(String),
}

struct RunningSklandProfileTask {
    kind: SklandProfileTaskKind,
    handle: JoinHandle<SklandProfileTaskFinished>,
}

enum SklandProfileTaskKind {
    InspectPlayerInfo,
    ImportStatusBuilding,
    ImportOperatorState,
}

impl SklandProfileTaskKind {
    fn label(&self) -> &'static str {
        match self {
            Self::InspectPlayerInfo => "后台任务：森空岛 player/info 检查中",
            Self::ImportStatusBuilding => "后台任务：森空岛账号/基建状态导入中",
            Self::ImportOperatorState => "后台任务：森空岛干员状态导入中",
        }
    }
}

enum SklandProfileTaskFinished {
    Inspect(Result<SklandPlayerInfoInspectOutcome, String>),
    ImportStatusBuilding(Result<SklandStatusBuildingImportOutcome, String>),
    Import(Result<SklandOperatorImportOutcome, String>),
}

struct SklandLoginTaskRequest {
    auth_file_path: PathBuf,
    uid_file_value: String,
}

struct SklandLoginTaskSuccess {
    auth_file_path: PathBuf,
    has_user_id: bool,
}

struct SklandAuthFileSummary {
    auth_file_path: PathBuf,
    exists: bool,
    uid_file: String,
    has_cred: bool,
    has_token: bool,
    has_user_id: bool,
    has_access_token: bool,
}

struct SklandAuthLoadOutcome {
    exists: bool,
    file: SklandLocalAuthFile,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct SklandLocalAuthFile {
    #[serde(default)]
    skland: SklandLocalAuthSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SklandLocalAuthSection {
    #[serde(default = "default_skland_uid_file_value")]
    uid_file: String,
    #[serde(default)]
    cred: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    access_token: String,
}

impl Default for SklandLocalAuthSection {
    fn default() -> Self {
        Self {
            uid_file: default_skland_uid_file_value(),
            cred: String::new(),
            token: String::new(),
            user_id: String::new(),
            access_token: String::new(),
        }
    }
}

#[derive(Debug)]
struct SklandResolvedAuth {
    access_token: String,
    cred: String,
    token: String,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SklandApiEnvelope<T> {
    status: Option<i64>,
    code: Option<i64>,
    msg: Option<String>,
    message: Option<String>,
    data: Option<T>,
}

impl<T> SklandApiEnvelope<T> {
    fn status_code(&self) -> Option<i64> {
        self.status.or(self.code)
    }

    fn message_text(&self) -> String {
        self.msg
            .as_deref()
            .or(self.message.as_deref())
            .unwrap_or("服务端没有返回可读消息")
            .to_string()
    }
}

#[derive(Debug, Serialize)]
struct SklandScanLoginRequest<'a> {
    #[serde(rename = "appCode")]
    app_code: &'a str,
}

#[derive(Debug, Deserialize)]
struct SklandScanLoginData {
    #[serde(rename = "scanId")]
    scan_id: String,
}

#[derive(Debug, Deserialize)]
struct SklandScanStatusData {
    #[serde(rename = "scanCode")]
    scan_code: Option<String>,
}

#[derive(Debug, Serialize)]
struct SklandTokenByScanCodeRequest<'a> {
    #[serde(rename = "scanCode")]
    scan_code: &'a str,
}

#[derive(Debug, Deserialize)]
struct SklandTokenByScanCodeData {
    token: String,
}

#[derive(Debug, Serialize)]
struct SklandGrantRequest<'a> {
    #[serde(rename = "appCode")]
    app_code: &'a str,
    token: &'a str,
    #[serde(rename = "type")]
    grant_type: i32,
}

#[derive(Debug, Deserialize)]
struct SklandGrantData {
    code: String,
}

#[derive(Debug, Serialize)]
struct SklandGenerateCredRequest<'a> {
    code: &'a str,
    kind: i32,
}

#[derive(Debug, Deserialize)]
struct SklandGenerateCredData {
    cred: String,
    token: String,
    #[serde(rename = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SklandRefreshTokenData {
    token: String,
}

fn default_skland_uid_file_value() -> String {
    "uid.txt".to_string()
}

fn default_skland_auth_file_path(working_directory: &Path) -> PathBuf {
    working_directory.join("skland-auth.local.toml")
}

fn build_skland_auth_file_summary(
    auth_file_path: &Path,
    loaded: &SklandAuthLoadOutcome,
) -> SklandAuthFileSummary {
    SklandAuthFileSummary {
        auth_file_path: auth_file_path.to_path_buf(),
        exists: loaded.exists,
        uid_file: loaded.file.skland.uid_file.clone(),
        has_cred: !loaded.file.skland.cred.trim().is_empty(),
        has_token: !loaded.file.skland.token.trim().is_empty(),
        has_user_id: !loaded.file.skland.user_id.trim().is_empty(),
        has_access_token: !loaded.file.skland.access_token.trim().is_empty(),
    }
}

fn read_skland_auth_file_or_default(path: &Path) -> Result<SklandAuthLoadOutcome, String> {
    if !path.exists() {
        return Ok(SklandAuthLoadOutcome {
            exists: false,
            file: SklandLocalAuthFile::default(),
        });
    }

    let document = fs::read_to_string(path)
        .map_err(|error| format!("读取森空岛鉴权文件 {} 失败：{error}", path.display()))?;
    let file = toml::from_str::<SklandLocalAuthFile>(&document)
        .map_err(|error| format!("解析森空岛鉴权文件 {} 失败：{error}", path.display()))?;

    Ok(SklandAuthLoadOutcome { exists: true, file })
}

fn write_skland_auth_file(
    path: &Path,
    uid_file_value: &str,
    auth: &SklandResolvedAuth,
) -> Result<(), String> {
    let mut loaded = read_skland_auth_file_or_default(path)?;
    loaded.file.skland.uid_file = if uid_file_value.trim().is_empty() {
        default_skland_uid_file_value()
    } else {
        uid_file_value.trim().to_string()
    };
    loaded.file.skland.cred = auth.cred.clone();
    loaded.file.skland.token = auth.token.clone();
    loaded.file.skland.user_id = auth.user_id.clone().unwrap_or_default();
    loaded.file.skland.access_token = auth.access_token.clone();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("创建森空岛鉴权文件目录 {} 失败：{error}", parent.display())
        })?;
    }

    let document = toml::to_string_pretty(&loaded.file)
        .map_err(|error| format!("序列化森空岛鉴权文件失败：{error}"))?;
    fs::write(path, document)
        .map_err(|error| format!("写入森空岛鉴权文件 {} 失败：{error}", path.display()))
}

fn load_skland_qr_texture(
    context: &egui::Context,
    qr_payload: &str,
) -> Result<(egui::TextureHandle, [usize; 2]), String> {
    let qr = QrCode::new(qr_payload.as_bytes())
        .map_err(|error| format!("构建森空岛二维码失败：{error}"))?;
    let module_width = qr.width();
    let image_width = (module_width + SKLAND_QR_QUIET_ZONE * 2) * SKLAND_QR_MODULE_SIZE;
    let mut rgba = vec![255_u8; image_width * image_width * 4];

    for y in 0..image_width {
        for x in 0..image_width {
            let module_x = x / SKLAND_QR_MODULE_SIZE;
            let module_y = y / SKLAND_QR_MODULE_SIZE;
            let inside_code = module_x >= SKLAND_QR_QUIET_ZONE
                && module_x < module_width + SKLAND_QR_QUIET_ZONE
                && module_y >= SKLAND_QR_QUIET_ZONE
                && module_y < module_width + SKLAND_QR_QUIET_ZONE;
            let is_dark = inside_code
                && matches!(
                    qr[(
                        module_x - SKLAND_QR_QUIET_ZONE,
                        module_y - SKLAND_QR_QUIET_ZONE
                    )],
                    QrColor::Dark
                );
            if is_dark {
                let offset = (y * image_width + x) * 4;
                rgba[offset] = 0;
                rgba[offset + 1] = 0;
                rgba[offset + 2] = 0;
                rgba[offset + 3] = 255;
            }
        }
    }

    let color_image = egui::ColorImage::from_rgba_unmultiplied([image_width, image_width], &rgba);
    let texture = context.load_texture(
        "skland-login-qr-texture",
        color_image,
        egui::TextureOptions::NEAREST,
    );

    Ok((texture, [image_width, image_width]))
}

fn run_skland_qr_login_task(
    request: SklandLoginTaskRequest,
    sender: mpsc::Sender<SklandLoginProgress>,
) -> Result<SklandLoginTaskSuccess, String> {
    let client = Client::builder()
        .user_agent(SKLAND_USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("初始化森空岛 HTTP 客户端失败：{error}"))?;

    send_skland_progress(
        &sender,
        SklandLoginProgress::Status("正在申请森空岛扫码会话".to_string()),
    );
    let scan_login = skland_post_json::<_, SklandScanLoginData>(
        &client,
        "https://as.hypergryph.com/general/v1/gen_scan/login",
        &SklandScanLoginRequest {
            app_code: SKLAND_APP_CODE,
        },
    )?;
    let scan_login = skland_require_success_data(scan_login, "申请森空岛扫码会话")?;
    let qr_payload = format!("hypergryph://scan_login?scanId={}", scan_login.scan_id);
    send_skland_progress(&sender, SklandLoginProgress::QrReady(qr_payload));

    let scan_code = poll_skland_scan_code(&client, &scan_login.scan_id, &sender)?;
    send_skland_progress(
        &sender,
        SklandLoginProgress::Status("扫码已确认，正在换取 access_token".to_string()),
    );
    let token_response = skland_post_json::<_, SklandTokenByScanCodeData>(
        &client,
        "https://as.hypergryph.com/user/auth/v1/token_by_scan_code",
        &SklandTokenByScanCodeRequest {
            scan_code: &scan_code,
        },
    )?;
    let access_token = skland_require_success_data(token_response, "换取 access_token")?.token;

    send_skland_progress(
        &sender,
        SklandLoginProgress::Status("正在换取森空岛授权 code".to_string()),
    );
    let grant_response = skland_post_json::<_, SklandGrantData>(
        &client,
        "https://as.hypergryph.com/user/oauth2/v2/grant",
        &SklandGrantRequest {
            app_code: SKLAND_APP_CODE,
            token: &access_token,
            grant_type: 0,
        },
    )?;
    let grant_code = skland_require_success_data(grant_response, "换取森空岛授权 code")?.code;

    send_skland_progress(
        &sender,
        SklandLoginProgress::Status("正在换取 cred / token".to_string()),
    );
    let cred_response = skland_post_json::<_, SklandGenerateCredData>(
        &client,
        "https://zonai.skland.com/api/v1/user/auth/generate_cred_by_code",
        &SklandGenerateCredRequest {
            code: &grant_code,
            kind: 1,
        },
    )?;
    let cred_data = skland_require_success_data(cred_response, "换取 cred / token")?;
    send_skland_progress(
        &sender,
        SklandLoginProgress::Status("正在刷新森空岛签名 token".to_string()),
    );
    let refresh_response = skland_get_json_with_headers::<SklandRefreshTokenData>(
        &client,
        "https://zonai.skland.com/api/v1/auth/refresh",
        &[],
        &[("cred", cred_data.cred.as_str())],
    )?;
    let refresh_data = skland_require_success_data(refresh_response, "刷新森空岛签名 token")?;
    let resolved = SklandResolvedAuth {
        access_token,
        cred: cred_data.cred,
        token: if refresh_data.token.trim().is_empty() {
            cred_data.token
        } else {
            refresh_data.token
        },
        user_id: cred_data
            .user_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    };

    write_skland_auth_file(&request.auth_file_path, &request.uid_file_value, &resolved)?;
    send_skland_progress(
        &sender,
        SklandLoginProgress::Status("登录成功，已写入本地鉴权文件".to_string()),
    );

    Ok(SklandLoginTaskSuccess {
        auth_file_path: request.auth_file_path,
        has_user_id: resolved.user_id.is_some(),
    })
}

fn run_skland_player_info_inspect_task(
    auth_file_path: PathBuf,
    database_path: PathBuf,
) -> Result<SklandPlayerInfoInspectOutcome, String> {
    let database = AppDatabase::open(&database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = SklandClient::new().map_err(|error| error.to_string())?;
    inspect_skland_player_info(
        &repository,
        &client,
        &SklandProfileRequest { auth_file_path },
    )
    .map_err(|error| error.to_string())
}

fn run_skland_status_building_import_task(
    auth_file_path: PathBuf,
    database_path: PathBuf,
) -> Result<SklandStatusBuildingImportOutcome, String> {
    let database = AppDatabase::open(&database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = SklandClient::new().map_err(|error| error.to_string())?;
    import_skland_player_info_into_status_and_building_state(
        &repository,
        &client,
        &SklandProfileRequest { auth_file_path },
    )
    .map_err(|error| error.to_string())
}

fn run_skland_operator_state_import_task(
    auth_file_path: PathBuf,
    database_path: PathBuf,
) -> Result<SklandOperatorImportOutcome, String> {
    let database = AppDatabase::open(&database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = SklandClient::new().map_err(|error| error.to_string())?;
    import_skland_player_info_into_operator_state(
        &repository,
        &client,
        &SklandProfileRequest { auth_file_path },
    )
    .map_err(|error| error.to_string())
}

fn poll_skland_scan_code(
    client: &Client,
    scan_id: &str,
    sender: &mpsc::Sender<SklandLoginProgress>,
) -> Result<String, String> {
    let mut last_status = String::new();

    for attempt in 0..SKLAND_SCAN_POLL_ATTEMPTS {
        let response = skland_get_json::<SklandScanStatusData>(
            client,
            "https://as.hypergryph.com/general/v1/scan_status",
            &[("scanId", scan_id)],
        )?;
        let response_status = response
            .status_code()
            .ok_or_else(|| "轮询扫码状态失败：服务端没有返回 status/code".to_string())?;
        match response_status {
            0 => {
                let scan_code = response
                    .data
                    .and_then(|data| data.scan_code)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "扫码已完成，但响应里没有可用的 scanCode".to_string())?;
                return Ok(scan_code);
            }
            100..=102 => {
                let status = format!("等待扫码确认：{}", response.message_text());
                if status != last_status {
                    send_skland_progress(sender, SklandLoginProgress::Status(status.clone()));
                    last_status = status;
                }
            }
            other => {
                return Err(format!(
                    "轮询扫码状态失败：{}（status {other}）",
                    response.message_text()
                ));
            }
        }

        if attempt + 1 < SKLAND_SCAN_POLL_ATTEMPTS {
            thread::sleep(SKLAND_SCAN_POLL_INTERVAL);
        }
    }

    Err("二维码已超时，请重新点击“确认登录”生成新的二维码".to_string())
}

fn send_skland_progress(sender: &mpsc::Sender<SklandLoginProgress>, progress: SklandLoginProgress) {
    let _ = sender.send(progress);
}

fn skland_post_json<TRequest, TResponse>(
    client: &Client,
    url: &str,
    body: &TRequest,
) -> Result<SklandApiEnvelope<TResponse>, String>
where
    TRequest: Serialize,
    TResponse: DeserializeOwned,
{
    let response = client
        .post(url)
        .json(body)
        .send()
        .map_err(|error| format!("请求 {url} 失败：{error}"))?;
    decode_skland_response(url, response)
}

fn skland_get_json<TResponse>(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<SklandApiEnvelope<TResponse>, String>
where
    TResponse: DeserializeOwned,
{
    let response = client
        .get(url)
        .query(query)
        .send()
        .map_err(|error| format!("请求 {url} 失败：{error}"))?;
    decode_skland_response(url, response)
}

fn skland_get_json_with_headers<TResponse>(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
) -> Result<SklandApiEnvelope<TResponse>, String>
where
    TResponse: DeserializeOwned,
{
    let mut request = client.get(url).query(query);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let response = request
        .send()
        .map_err(|error| format!("请求 {url} 失败：{error}"))?;
    decode_skland_response(url, response)
}

fn decode_skland_response<TResponse>(
    url: &str,
    response: reqwest::blocking::Response,
) -> Result<SklandApiEnvelope<TResponse>, String>
where
    TResponse: DeserializeOwned,
{
    let http_status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<unknown>")
        .to_string();
    let body = response
        .text()
        .map_err(|error| format!("读取 {url} 响应体失败：{error}"))?;

    if !http_status.is_success() {
        return Err(format!(
            "请求 {url} 失败：HTTP {}；Content-Type {content_type}；响应摘要：{}",
            http_status.as_u16(),
            summarize_skland_response_body(&body)
        ));
    }

    serde_json::from_str::<SklandApiEnvelope<TResponse>>(&body).map_err(|error| {
        format!(
            "解析 {url} 响应失败：{error}；Content-Type {content_type}；响应摘要：{}",
            summarize_skland_response_body(&body)
        )
    })
}

fn summarize_skland_response_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "响应体为空".to_string();
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let mut parts = Vec::new();

        if let Some(status) = value.get("status").and_then(|value| value.as_i64()) {
            parts.push(format!("status={status}"));
        }
        if let Some(code) = value.get("code").and_then(|value| value.as_i64()) {
            parts.push(format!("code={code}"));
        }
        if let Some(msg) = value.get("msg").and_then(|value| value.as_str()) {
            parts.push(format!("msg={msg}"));
        }
        if let Some(message) = value.get("message").and_then(|value| value.as_str()) {
            parts.push(format!("message={message}"));
        }
        if let Some(data) = value.get("data") {
            match data {
                serde_json::Value::Object(map) => {
                    let mut keys = map.keys().cloned().collect::<Vec<_>>();
                    keys.sort();
                    parts.push(format!("data_keys={}", keys.join(",")));
                }
                serde_json::Value::Array(items) => {
                    parts.push(format!("data_len={}", items.len()));
                }
                serde_json::Value::Null => {
                    parts.push("data=null".to_string());
                }
                _ => {
                    parts.push("data=<scalar>".to_string());
                }
            }
        }

        if parts.is_empty() {
            if let Some(object) = value.as_object() {
                let mut keys = object.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                parts.push(format!("top_level_keys={}", keys.join(",")));
            } else {
                let json_type = match value {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "bool",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => "object",
                };
                parts.push(format!("json_type={json_type}"));
            }
        }

        return parts.join("；");
    }

    format!("响应体不是可解析 JSON，长度 {} 字节", trimmed.len())
}

fn skland_require_success_data<T>(
    response: SklandApiEnvelope<T>,
    operation: &str,
) -> Result<T, String> {
    let response_status = response
        .status_code()
        .ok_or_else(|| format!("{operation}失败：服务端没有返回 status/code"))?;
    if response_status != 0 {
        return Err(format!(
            "{operation}失败：{}（status {}）",
            response.message_text(),
            response_status
        ));
    }

    response
        .data
        .ok_or_else(|| format!("{operation}失败：服务端没有返回 data"))
}

struct SettingsPageState {
    config_path: PathBuf,
    source_description_zh: String,
    form: ConfigForm,
    saved_form: ConfigForm,
    notice: Option<UiNotice>,
}

impl SettingsPageState {
    fn from_loaded(loaded: &LoadedConfig) -> Self {
        let form = ConfigForm::from_config(&loaded.config);
        Self {
            config_path: loaded.save_path().to_path_buf(),
            source_description_zh: describe_config_source_zh(&loaded.source),
            form: form.clone(),
            saved_form: form,
            notice: None,
        }
    }

    fn is_dirty(&self) -> bool {
        self.form != self.saved_form
    }

    fn reload(&mut self) -> Result<String, String> {
        let loaded = AppConfig::load_or_default_from(self.config_path.clone())
            .map_err(|error| error.to_string())?;
        let form = ConfigForm::from_config(&loaded.config);
        self.config_path = loaded.save_path().to_path_buf();
        self.source_description_zh = describe_config_source_zh(&loaded.source);
        self.form = form.clone();
        self.saved_form = form;

        Ok(format!("已从 {} 重载配置", self.config_path.display()))
    }

    fn save(&mut self) -> Result<SaveOutcome, String> {
        let logging_changed = self.form.log_directory != self.saved_form.log_directory
            || self.form.log_file_name != self.saved_form.log_file_name;
        let config = self.form.to_config();
        let path = config
            .save_to_path(self.config_path.clone())
            .map_err(|error| error.to_string())?;

        self.config_path = path.clone();
        self.source_description_zh = format!("文件 {}", path.display());
        self.saved_form = self.form.clone();

        Ok(SaveOutcome {
            path,
            logging_changed,
        })
    }
}

struct SyncPageState {
    database_path_text: String,
    force_full_sync: bool,
    selected_tab: SyncTab,
    prts: SourceSyncOverview,
    prts_items: PrtsItemIndexOverview,
    prts_operators: PrtsOperatorIndexOverview,
    prts_building_skills: PrtsOperatorBuildingSkillOverview,
    prts_growth: PrtsOperatorGrowthOverview,
    prts_recipes: PrtsRecipeIndexOverview,
    prts_stages: PrtsStageIndexOverview,
    official: OfficialNoticeSyncOverview,
    penguin: PenguinSyncOverview,
    notice: Option<UiNotice>,
    running_task: Option<RunningSyncTask>,
}

impl SyncPageState {
    fn new(working_directory: &Path) -> Self {
        let database_path = default_database_path(working_directory);
        let mut state = Self {
            database_path_text: database_path.display().to_string(),
            force_full_sync: false,
            selected_tab: SyncTab::Prts,
            prts: SourceSyncOverview::empty(PRTS_SITEINFO_SOURCE_ID, PRTS_SITEINFO_CACHE_KEY),
            prts_items: PrtsItemIndexOverview::empty(),
            prts_operators: PrtsOperatorIndexOverview::empty(),
            prts_building_skills: PrtsOperatorBuildingSkillOverview::empty(),
            prts_growth: PrtsOperatorGrowthOverview::empty(),
            prts_recipes: PrtsRecipeIndexOverview::empty(),
            prts_stages: PrtsStageIndexOverview::empty(),
            official: OfficialNoticeSyncOverview::empty(),
            penguin: PenguinSyncOverview::empty(),
            notice: None,
            running_task: None,
        };

        if let Err(error) = state.refresh_from_database() {
            state.notice = Some(UiNotice::warning(format!("初始化同步页失败：{error}")));
        }

        state
    }

    fn is_running(&self) -> bool {
        self.running_task.is_some()
    }

    fn running_label(&self) -> Option<&'static str> {
        self.running_task
            .as_ref()
            .map(|task| task.kind.label(task.mode))
    }

    fn selected_sync_mode(&self) -> SyncMode {
        if self.force_full_sync {
            SyncMode::Full
        } else {
            SyncMode::Incremental
        }
    }

    fn refresh_from_database(&mut self) -> Result<(), String> {
        let database_path = self.database_path()?;
        let database = AppDatabase::open(&database_path).map_err(|error| error.to_string())?;
        let repository = AppRepository::new(database.connection());

        self.prts = SourceSyncOverview::load(
            &repository,
            PRTS_SITEINFO_SOURCE_ID,
            PRTS_SITEINFO_CACHE_KEY,
        )?;
        self.prts_items = PrtsItemIndexOverview::load(&repository)?;
        self.prts_operators = PrtsOperatorIndexOverview::load(&repository)?;
        self.prts_building_skills = PrtsOperatorBuildingSkillOverview::load(&repository)?;
        self.prts_growth = PrtsOperatorGrowthOverview::load(&repository)?;
        self.prts_recipes = PrtsRecipeIndexOverview::load(&repository)?;
        self.prts_stages = PrtsStageIndexOverview::load(&repository)?;
        self.official = OfficialNoticeSyncOverview::load(&repository)?;
        self.penguin = PenguinSyncOverview::load(&repository)?;

        Ok(())
    }

    fn start_prts_sync(&mut self, working_directory: &Path) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有同步任务正在后台执行，请等待完成"));
            return;
        }

        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };
        let working_directory = working_directory.to_path_buf();
        let mode = self.selected_sync_mode();

        self.notice = Some(UiNotice::info(format!(
            "PRTS {}同步已开始，正在后台执行",
            mode.label_zh()
        )));
        self.selected_tab = SyncTab::Prts;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::Prts,
            mode,
            handle: thread::spawn(move || {
                SyncTaskFinished::Prts(Box::new(run_prts_sync_task(
                    &database_path,
                    &working_directory,
                    mode,
                )))
            }),
        });
    }

    fn start_prts_growth_sync(&mut self) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有同步任务正在后台执行，请等待完成"));
            return;
        }

        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };
        let mode = self.selected_sync_mode();

        self.notice = Some(UiNotice::info(format!(
            "PRTS 养成需求 {}同步已开始，正在后台执行",
            mode.label_zh()
        )));
        self.selected_tab = SyncTab::Prts;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::PrtsGrowth,
            mode,
            handle: thread::spawn(move || {
                SyncTaskFinished::PrtsGrowth(run_prts_growth_sync_task(&database_path, mode))
            }),
        });
    }

    fn start_prts_building_skill_sync(&mut self) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有同步任务正在后台执行，请等待完成"));
            return;
        }

        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };
        let mode = self.selected_sync_mode();

        self.notice = Some(UiNotice::info(format!(
            "PRTS 基建技能 {}同步已开始，正在后台执行",
            mode.label_zh()
        )));
        self.selected_tab = SyncTab::Prts;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::PrtsBuildingSkill,
            mode,
            handle: thread::spawn(move || {
                SyncTaskFinished::PrtsBuildingSkill(run_prts_building_skill_sync_task(
                    &database_path,
                    mode,
                ))
            }),
        });
    }

    fn start_official_sync(&mut self) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有同步任务正在后台执行，请等待完成"));
            return;
        }

        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };
        let mode = self.selected_sync_mode();

        self.notice = Some(UiNotice::info(format!(
            "官方公告 {}同步已开始，正在后台执行",
            mode.label_zh()
        )));
        self.selected_tab = SyncTab::Official;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::Official,
            mode,
            handle: thread::spawn(move || {
                SyncTaskFinished::Official(run_official_notice_sync_task(&database_path, mode))
            }),
        });
    }

    fn start_penguin_sync(&mut self) {
        if self.is_running() {
            self.notice = Some(UiNotice::warning("已有同步任务正在后台执行，请等待完成"));
            return;
        }

        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(UiNotice::error(error));
                return;
            }
        };
        let mode = self.selected_sync_mode();

        self.notice = Some(UiNotice::info(format!(
            "Penguin {}同步已开始，正在后台执行",
            mode.label_zh()
        )));
        self.selected_tab = SyncTab::Penguin;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::Penguin,
            mode,
            handle: thread::spawn(move || {
                SyncTaskFinished::Penguin(run_penguin_sync_task(&database_path, mode))
            }),
        });
    }

    fn poll_running_task(&mut self) {
        if !self
            .running_task
            .as_ref()
            .is_some_and(|task| task.handle.is_finished())
        {
            return;
        }

        let task = self
            .running_task
            .take()
            .expect("finished task should exist");
        let joined = task.handle.join();
        let refresh_result = self.refresh_from_database();

        match joined {
            Ok(SyncTaskFinished::Prts(result)) => match *result {
                Ok(outcome) => {
                    tracing::info!(
                        site_revision = %outcome.site_info.revision,
                        operator_count = outcome.operator_index.row_count,
                        item_count = outcome.item_index.row_count,
                        stage_count = outcome.stage_index.row_count,
                        recipe_count = outcome.recipe_index.row_count,
                        "desktop prts sync completed"
                    );
                    self.notice = Some(build_sync_success_notice(
                        "PRTS",
                        format!(
                            "请求模式：{}；站点版本：{}；干员 {} 条（{} / {}）；道具 {} 条（{} / {}）；关卡 {} 条（{} / {}）；配方 {} 条（{} / {}）。",
                            task.mode.label_zh(),
                            outcome.site_info.revision,
                            outcome.operator_index.row_count,
                            outcome.operator_index.effective_mode.label_zh(),
                            outcome.operator_index.run_status.label_zh(),
                            outcome.item_index.row_count,
                            outcome.item_index.effective_mode.label_zh(),
                            outcome.item_index.run_status.label_zh(),
                            outcome.stage_index.row_count,
                            outcome.stage_index.effective_mode.label_zh(),
                            outcome.stage_index.run_status.label_zh(),
                            outcome.recipe_index.row_count,
                            outcome.recipe_index.effective_mode.label_zh(),
                            outcome.recipe_index.run_status.label_zh()
                        ),
                        refresh_result,
                    ));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop prts sync failed");
                    self.notice = Some(build_sync_error_notice("PRTS", error, refresh_result));
                }
            },
            Ok(SyncTaskFinished::PrtsGrowth(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        source_id = %outcome.source_id,
                        revision = %outcome.revision,
                        row_count = outcome.row_count,
                        "desktop prts growth sync completed"
                    );
                    self.notice = Some(build_sync_success_notice(
                        "PRTS 养成需求",
                        format!(
                            "请求模式：{}；实际执行：{}；结果：{}；写入 {} 条养成需求记录，版本锚点：{}。",
                            task.mode.label_zh(),
                            outcome.effective_mode.label_zh(),
                            outcome.run_status.label_zh(),
                            outcome.row_count,
                            outcome.revision
                        ),
                        refresh_result,
                    ));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop prts growth sync failed");
                    self.notice = Some(build_sync_error_notice(
                        "PRTS 养成需求",
                        error,
                        refresh_result,
                    ));
                }
            },
            Ok(SyncTaskFinished::PrtsBuildingSkill(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        source_id = %outcome.source_id,
                        revision = %outcome.revision,
                        row_count = outcome.row_count,
                        "desktop prts building skill sync completed"
                    );
                    self.notice = Some(build_sync_success_notice(
                        "PRTS 基建技能",
                        format!(
                            "请求模式：{}；实际执行：{}；结果：{}；写入 {} 条基建技能记录，版本锚点：{}。",
                            task.mode.label_zh(),
                            outcome.effective_mode.label_zh(),
                            outcome.run_status.label_zh(),
                            outcome.row_count,
                            outcome.revision
                        ),
                        refresh_result,
                    ));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop prts building skill sync failed");
                    self.notice = Some(build_sync_error_notice(
                        "PRTS 基建技能",
                        error,
                        refresh_result,
                    ));
                }
            },
            Ok(SyncTaskFinished::Official(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        source_id = %outcome.source_id,
                        revision = %outcome.revision,
                        row_count = outcome.row_count,
                        "desktop official notice sync completed"
                    );
                    self.notice = Some(build_sync_success_notice(
                        "官方公告",
                        format!(
                            "请求模式：{}；实际执行：{}；结果：{}；写入 {} 条官方公告记录，版本锚点：{}。",
                            task.mode.label_zh(),
                            outcome.effective_mode.label_zh(),
                            outcome.run_status.label_zh(),
                            outcome.row_count,
                            outcome.revision
                        ),
                        refresh_result,
                    ));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop official notice sync failed");
                    self.notice = Some(build_sync_error_notice("官方公告", error, refresh_result));
                }
            },
            Ok(SyncTaskFinished::Penguin(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        source_id = %outcome.source_id,
                        revision = %outcome.revision,
                        row_count = outcome.row_count,
                        "desktop penguin sync completed"
                    );
                    self.notice = Some(build_sync_success_notice(
                        "Penguin",
                        format!(
                            "请求模式：{}；实际执行：{}；结果：{}；写入 {} 条矩阵记录，版本锚点：{}。",
                            task.mode.label_zh(),
                            outcome.effective_mode.label_zh(),
                            outcome.run_status.label_zh(),
                            outcome.row_count,
                            outcome.revision
                        ),
                        refresh_result,
                    ));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop penguin sync failed");
                    self.notice = Some(build_sync_error_notice("Penguin", error, refresh_result));
                }
            },
            Err(_) => {
                self.notice = Some(UiNotice::error(format!(
                    "{} 任务线程异常退出",
                    task.kind.display_name()
                )));
            }
        }
    }

    fn database_path(&self) -> Result<PathBuf, String> {
        let trimmed = self.database_path_text.trim();
        if trimmed.is_empty() {
            Err("数据库路径不能为空".to_string())
        } else {
            Ok(PathBuf::from(trimmed))
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum SyncTab {
    Prts,
    Official,
    Penguin,
}

struct RunningSyncTask {
    kind: SyncTaskKind,
    mode: SyncMode,
    handle: JoinHandle<SyncTaskFinished>,
}

#[derive(Copy, Clone)]
enum SyncTaskKind {
    Prts,
    PrtsGrowth,
    PrtsBuildingSkill,
    Official,
    Penguin,
}

impl SyncTaskKind {
    fn label(self, mode: SyncMode) -> &'static str {
        match self {
            Self::Prts => match mode {
                SyncMode::Incremental => "后台任务：PRTS 增量同步中",
                SyncMode::Full => "后台任务：PRTS 全量同步中",
            },
            Self::PrtsGrowth => match mode {
                SyncMode::Incremental => "后台任务：PRTS 养成需求增量请求处理中",
                SyncMode::Full => "后台任务：PRTS 养成需求全量同步中",
            },
            Self::PrtsBuildingSkill => match mode {
                SyncMode::Incremental => "后台任务：PRTS 基建技能增量请求处理中",
                SyncMode::Full => "后台任务：PRTS 基建技能全量同步中",
            },
            Self::Official => match mode {
                SyncMode::Incremental => "后台任务：官方公告增量请求处理中",
                SyncMode::Full => "后台任务：官方公告全量同步中",
            },
            Self::Penguin => match mode {
                SyncMode::Incremental => "后台任务：Penguin 增量同步中",
                SyncMode::Full => "后台任务：Penguin 全量同步中",
            },
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Prts => "PRTS 同步",
            Self::PrtsGrowth => "PRTS 养成需求同步",
            Self::PrtsBuildingSkill => "PRTS 基建技能同步",
            Self::Official => "官方公告同步",
            Self::Penguin => "Penguin 同步",
        }
    }
}

enum SyncTaskFinished {
    Prts(Box<Result<SyncPrtsOutcome, String>>),
    PrtsGrowth(Result<SyncPrtsOperatorGrowthOutcome, String>),
    PrtsBuildingSkill(Result<SyncPrtsOperatorBuildingSkillOutcome, String>),
    Official(Result<SyncOfficialNoticeOutcome, String>),
    Penguin(Result<SyncPenguinMatrixOutcome, String>),
}

#[derive(Clone)]
struct SourceSyncOverview {
    status: Option<String>,
    last_attempt_at: Option<String>,
    last_success_at: Option<String>,
    revision: Option<String>,
    cache_bytes: Option<i64>,
    fetched_at: Option<String>,
    last_error: Option<String>,
}

impl SourceSyncOverview {
    fn empty(_source_id: &'static str, _cache_key: &'static str) -> Self {
        Self {
            status: None,
            last_attempt_at: None,
            last_success_at: None,
            revision: None,
            cache_bytes: None,
            fetched_at: None,
            last_error: None,
        }
    }

    fn load(
        repository: &AppRepository<'_>,
        source_id: &'static str,
        cache_key: &'static str,
    ) -> Result<Self, String> {
        let state = repository
            .get_sync_source_state(source_id)
            .map_err(|error| error.to_string())?;
        let cache = repository
            .get_raw_source_cache_summary(cache_key)
            .map_err(|error| error.to_string())?;

        Ok(Self::from_records(state, cache))
    }

    fn from_records(
        state: Option<SyncSourceStateRecord>,
        cache: Option<RawSourceCacheSummary>,
    ) -> Self {
        Self {
            status: state.as_ref().map(|value| value.status.clone()),
            last_attempt_at: state
                .as_ref()
                .and_then(|value| value.last_attempt_at.clone()),
            last_success_at: state
                .as_ref()
                .and_then(|value| value.last_success_at.clone()),
            revision: cache.as_ref().and_then(|value| value.revision.clone()),
            cache_bytes: cache.as_ref().map(|value| value.payload_bytes),
            fetched_at: cache.as_ref().map(|value| value.fetched_at.clone()),
            last_error: state.and_then(|value| value.last_error),
        }
    }

    fn status_label(&self) -> &str {
        self.status.as_deref().unwrap_or("尚未同步")
    }
}

#[derive(Clone)]
struct PrtsItemIndexOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalItemDefRecord>,
}

impl PrtsItemIndexOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(PRTS_ITEM_INDEX_SOURCE_ID, PRTS_ITEM_INDEX_CACHE_KEY),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PRTS_ITEM_INDEX_SOURCE_ID,
                PRTS_ITEM_INDEX_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_item_defs()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_item_defs(8)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Clone)]
struct PrtsOperatorIndexOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalOperatorDefRecord>,
}

impl PrtsOperatorIndexOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(
                PRTS_OPERATOR_INDEX_SOURCE_ID,
                PRTS_OPERATOR_INDEX_CACHE_KEY,
            ),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PRTS_OPERATOR_INDEX_SOURCE_ID,
                PRTS_OPERATOR_INDEX_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_operator_defs()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_operator_defs(8)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Clone)]
struct PrtsOperatorBuildingSkillOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalOperatorBuildingSkillRecord>,
}

impl PrtsOperatorBuildingSkillOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(
                PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID,
                PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY,
            ),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID,
                PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_operator_building_skills()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_operator_building_skills(12)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Clone)]
struct PrtsOperatorGrowthOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalOperatorGrowthRecord>,
}

impl PrtsOperatorGrowthOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(
                PRTS_OPERATOR_GROWTH_SOURCE_ID,
                PRTS_OPERATOR_GROWTH_CACHE_KEY,
            ),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PRTS_OPERATOR_GROWTH_SOURCE_ID,
                PRTS_OPERATOR_GROWTH_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_operator_growths()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_operator_growths(24)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrtsOperatorGrowthDisplayRow {
    operator_name_zh: String,
    stage_label: String,
    material_slot: String,
    material_summary: String,
}

#[derive(Clone)]
struct PrtsRecipeIndexOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalRecipeRecord>,
}

impl PrtsRecipeIndexOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(
                PRTS_RECIPE_INDEX_SOURCE_ID,
                PRTS_RECIPE_INDEX_CACHE_KEY,
            ),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PRTS_RECIPE_INDEX_SOURCE_ID,
                PRTS_RECIPE_INDEX_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_recipes()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_recipes(8)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Clone)]
struct PrtsStageIndexOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalStageDefRecord>,
}

impl PrtsStageIndexOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(
                PRTS_STAGE_INDEX_SOURCE_ID,
                PRTS_STAGE_INDEX_CACHE_KEY,
            ),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PRTS_STAGE_INDEX_SOURCE_ID,
                PRTS_STAGE_INDEX_CACHE_KEY,
            )?,
            row_count: repository
                .count_prts_stage_defs()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_prts_stage_defs(8)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Clone)]
struct PenguinSyncOverview {
    source: SourceSyncOverview,
    row_count: i64,
    current_stage_count: usize,
    sample_stages: Vec<PenguinStageDisplay>,
}

impl PenguinSyncOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(PENGUIN_MATRIX_SOURCE_ID, PENGUIN_MATRIX_CACHE_KEY),
            row_count: 0,
            current_stage_count: 0,
            sample_stages: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        let drop_rows = repository
            .list_penguin_drop_display_records()
            .map_err(|error| error.to_string())?;
        let stage_summaries = build_penguin_stage_displays(&drop_rows);

        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PENGUIN_MATRIX_SOURCE_ID,
                PENGUIN_MATRIX_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_drop_matrix()
                .map_err(|error| error.to_string())?,
            current_stage_count: stage_summaries.len(),
            sample_stages: stage_summaries.into_iter().take(12).collect(),
        })
    }
}

#[derive(Clone)]
struct PenguinStageDisplay {
    stage_name: String,
    stage_code: String,
    ap_cost: Option<i64>,
    recent_upload_count: i64,
    is_priority_stage: bool,
    normal_drops: Vec<PenguinDropDisplay>,
    special_drops: Vec<PenguinDropDisplay>,
}

#[derive(Clone)]
struct PenguinDropDisplay {
    item_name: String,
    probability: f64,
    expected_ap: Option<f64>,
}

#[derive(Clone)]
struct OfficialNoticeSyncOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalEventNoticeRecord>,
}

impl OfficialNoticeSyncOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(OFFICIAL_NOTICE_SOURCE_ID, OFFICIAL_NOTICE_CACHE_KEY),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                OFFICIAL_NOTICE_SOURCE_ID,
                OFFICIAL_NOTICE_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_event_notices()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_event_notices(8)
                .map_err(|error| error.to_string())?,
        })
    }
}

fn run_prts_sync_task(
    database_path: &Path,
    working_directory: &Path,
    mode: SyncMode,
) -> Result<SyncPrtsOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = PrtsClient::new().map_err(|error| error.to_string())?;
    sync_prts_with_mode(&repository, &client, working_directory, mode)
        .map_err(|error| error.to_string())
}

fn run_prts_growth_sync_task(
    database_path: &Path,
    mode: SyncMode,
) -> Result<SyncPrtsOperatorGrowthOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = PrtsClient::new().map_err(|error| error.to_string())?;
    sync_prts_operator_growth_with_mode(&repository, &client, mode)
        .map_err(|error| error.to_string())
}

fn run_prts_building_skill_sync_task(
    database_path: &Path,
    mode: SyncMode,
) -> Result<SyncPrtsOperatorBuildingSkillOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = PrtsClient::new().map_err(|error| error.to_string())?;
    sync_prts_operator_building_skill_with_mode(&repository, &client, mode)
        .map_err(|error| error.to_string())
}

fn run_official_notice_sync_task(
    database_path: &Path,
    mode: SyncMode,
) -> Result<SyncOfficialNoticeOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = OfficialNoticeClient::new().map_err(|error| error.to_string())?;
    sync_official_notices_with_mode(&repository, &client, mode).map_err(|error| error.to_string())
}

fn run_penguin_sync_task(
    database_path: &Path,
    mode: SyncMode,
) -> Result<SyncPenguinMatrixOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = PenguinClient::new().map_err(|error| error.to_string())?;
    sync_penguin_matrix_with_mode(&repository, &client, mode).map_err(|error| error.to_string())
}

fn build_sync_success_notice(
    source_name: &str,
    detail: String,
    refresh_result: Result<(), String>,
) -> UiNotice {
    match refresh_result {
        Ok(()) => UiNotice::success(format!("{source_name} 同步完成。{detail}")),
        Err(refresh_error) => UiNotice::warning(format!(
            "{source_name} 同步完成，但刷新本地概览失败：{refresh_error}。{detail}"
        )),
    }
}

fn build_sync_error_notice(
    source_name: &str,
    error: String,
    refresh_result: Result<(), String>,
) -> UiNotice {
    match refresh_result {
        Ok(()) => UiNotice::error(format!("{source_name} 同步失败：{error}")),
        Err(refresh_error) => UiNotice::error(format!(
            "{source_name} 同步失败：{error}；同时刷新本地概览失败：{refresh_error}"
        )),
    }
}

fn render_source_overview(
    ui: &mut egui::Ui,
    grid_id: &str,
    overview: &SourceSyncOverview,
    timezone: &str,
) {
    ui.label("这里保留用户真正关心的同步状态、时间、缓存体量和最近错误。");
    ui.separator();

    render_source_overview_grid(ui, grid_id, overview, timezone);
}

fn render_source_overview_grid(
    ui: &mut egui::Ui,
    grid_id: &str,
    overview: &SourceSyncOverview,
    timezone: &str,
) {
    egui::Grid::new(grid_id)
        .num_columns(2)
        .spacing([24.0, 12.0])
        .show(ui, |ui| {
            overview_row(ui, "同步状态", overview.status_label());
            overview_row(
                ui,
                "最后尝试",
                format_optional_datetime(&overview.last_attempt_at, timezone),
            );
            overview_row(
                ui,
                "最近成功",
                format_optional_datetime(&overview.last_success_at, timezone),
            );
            overview_row(
                ui,
                "版本锚点",
                format_optional_datetime(&overview.revision, timezone),
            );
            overview_row(
                ui,
                "缓存大小",
                optional_number(overview.cache_bytes, "字节"),
            );
            overview_row(
                ui,
                "缓存时间",
                format_optional_datetime(&overview.fetched_at, timezone),
            );
            overview_row(ui, "最近错误", optional_text(&overview.last_error));
        });
}

struct PrtsOverviewRefs<'a> {
    site_info: &'a SourceSyncOverview,
    item_index: &'a PrtsItemIndexOverview,
    operator_index: &'a PrtsOperatorIndexOverview,
    operator_building_skill: &'a PrtsOperatorBuildingSkillOverview,
    operator_growth: &'a PrtsOperatorGrowthOverview,
    recipe_index: &'a PrtsRecipeIndexOverview,
    stage_index: &'a PrtsStageIndexOverview,
}

fn render_prts_overview(ui: &mut egui::Ui, overview: PrtsOverviewRefs<'_>, timezone: &str) {
    ui.label(
        "PRTS 当前按七类数据展示：站点信息负责版本锚点，干员索引用于落地 `external_operator_def`（仅保留玩家 box 可拥有、可养成的正式干员；`专属干员` 与 `预备干员` 会被过滤），基建技能用于落地 `external_operator_building_skill`，养成需求用于落地 `external_operator_growth`，道具索引用于落地 `external_item_def`，配方索引用于落地 `external_recipe`，关卡索引用于补齐 `external_stage_def` 的静态定义。",
    );
    ui.label(
        "当前入口已拆分：上方“同步 PRTS”只同步站点 / 干员 / 道具 / 关卡 / 配方；“同步 PRTS 养成需求”和“同步 PRTS 基建技能”分别单独负责 growth 与 building skill。当前增量策略里，道具 / 干员 / 关卡 / 配方支持 `revision` 预检查，未变化时会跳过；养成需求与基建技能都来自单干员页 section，暂无稳定轻量锚点，因此仍按全量执行。",
    );
    ui.separator();

    ui.strong("站点信息");
    render_source_overview_grid(ui, "prts_siteinfo_overview", overview.site_info, timezone);
    ui.separator();

    ui.strong("干员索引");
    render_source_overview_grid(
        ui,
        "prts_operator_index_overview",
        &overview.operator_index.source,
        timezone,
    );
    ui.separator();
    ui.label(format!(
        "当前干员定义数：{}",
        overview.operator_index.row_count
    ));

    if overview.operator_index.sample_rows.is_empty() {
        ui.label("本地数据库里还没有 PRTS 干员定义记录。");
    } else {
        ui.label("示例干员定义：");
        egui::Grid::new("prts_operator_index_preview")
            .num_columns(5)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("干员 ID");
                ui.strong("名称");
                ui.strong("稀有度");
                ui.strong("职业");
                ui.strong("分支");
                ui.end_row();

                for row in &overview.operator_index.sample_rows {
                    ui.label(row.operator_id.as_str());
                    ui.label(row.name_zh.as_str());
                    ui.label(row.rarity.to_string());
                    ui.label(row.profession.as_str());
                    ui.label(row.branch.as_deref().unwrap_or("暂无"));
                    ui.end_row();
                }
            });
    }

    ui.separator();
    ui.strong("基建技能");
    render_source_overview_grid(
        ui,
        "prts_operator_building_skill_overview",
        &overview.operator_building_skill.source,
        timezone,
    );
    ui.separator();
    ui.label(format!(
        "当前基建技能行数：{}",
        overview.operator_building_skill.row_count
    ));

    if overview.operator_building_skill.sample_rows.is_empty() {
        ui.label("本地数据库里还没有 PRTS 基建技能记录。");
    } else {
        ui.label("示例基建技能：");
        egui::Grid::new("prts_operator_building_skill_preview")
            .num_columns(5)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("干员");
                ui.strong("解锁");
                ui.strong("房间");
                ui.strong("技能");
                ui.strong("效果");
                ui.end_row();

                for row in overview.operator_building_skill.sample_rows.iter().take(8) {
                    ui.label(row.operator_name_zh.as_str());
                    ui.label(row.condition_label.as_str());
                    ui.label(row.room_type_label.as_str());
                    ui.label(row.skill_name.as_str());
                    ui.label(if row.description.is_empty() {
                        "暂无".to_string()
                    } else {
                        row.description.clone()
                    });
                    ui.end_row();
                }
            });
    }

    ui.separator();
    ui.strong("养成需求");
    render_source_overview_grid(
        ui,
        "prts_operator_growth_overview",
        &overview.operator_growth.source,
        timezone,
    );
    ui.separator();
    ui.label(format!(
        "当前养成需求行数：{}",
        overview.operator_growth.row_count
    ));

    let growth_display_rows =
        build_prts_operator_growth_display_rows(&overview.operator_growth.sample_rows);

    if growth_display_rows.is_empty() {
        ui.label("本地数据库里还没有 PRTS 养成需求记录。");
    } else {
        ui.label("示例养成需求：");
        egui::Grid::new("prts_operator_growth_preview")
            .num_columns(4)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("干员");
                ui.strong("阶段");
                ui.strong("槽位");
                ui.strong("材料");
                ui.end_row();

                for row in growth_display_rows.iter().take(8) {
                    ui.label(row.operator_name_zh.as_str());
                    ui.label(row.stage_label.as_str());
                    ui.label(row.material_slot.as_str());
                    ui.label(if row.material_summary.is_empty() {
                        "暂无".to_string()
                    } else {
                        row.material_summary.clone()
                    });
                    ui.end_row();
                }
            });
    }

    ui.separator();
    ui.strong("道具索引");
    render_source_overview_grid(
        ui,
        "prts_item_index_overview",
        &overview.item_index.source,
        timezone,
    );
    ui.separator();
    ui.label(format!("当前道具定义数：{}", overview.item_index.row_count));

    if overview.item_index.sample_rows.is_empty() {
        ui.label("本地数据库里还没有 PRTS 道具定义记录。");
    } else {
        ui.label("示例道具定义：");
        egui::Grid::new("prts_item_index_preview")
            .num_columns(4)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("道具 ID");
                ui.strong("中文名");
                ui.strong("类型");
                ui.strong("稀有度");
                ui.end_row();

                for row in &overview.item_index.sample_rows {
                    ui.label(row.item_id.as_str());
                    ui.label(row.name_zh.as_str());
                    ui.label(row.item_type.as_str());
                    ui.label(
                        row.rarity
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "暂无".to_string()),
                    );
                    ui.end_row();
                }
            });
    }

    ui.separator();
    ui.strong("配方索引");
    render_source_overview_grid(
        ui,
        "prts_recipe_index_overview",
        &overview.recipe_index.source,
        timezone,
    );
    ui.separator();
    ui.label(format!(
        "当前 PRTS 配方数：{}",
        overview.recipe_index.row_count
    ));

    if overview.recipe_index.sample_rows.is_empty() {
        ui.label("本地数据库里还没有 PRTS 配方记录。");
    } else {
        ui.label("示例配方定义：");
        egui::Grid::new("prts_recipe_index_preview")
            .num_columns(4)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("产物");
                ui.strong("加工站等级");
                ui.strong("分类");
                ui.strong("原料");
                ui.end_row();

                for row in &overview.recipe_index.sample_rows {
                    ui.label(row.output_name_zh.as_str());
                    ui.label(row.workshop_level.to_string());
                    ui.label(row.recipe_kind.as_str());
                    ui.label(if row.ingredient_summary.is_empty() {
                        "暂无".to_string()
                    } else {
                        row.ingredient_summary.clone()
                    });
                    ui.end_row();
                }
            });
    }

    ui.separator();
    ui.strong("关卡索引");
    render_source_overview_grid(
        ui,
        "prts_stage_index_overview",
        &overview.stage_index.source,
        timezone,
    );
    ui.separator();
    ui.label(format!(
        "当前 PRTS 关卡定义数：{}",
        overview.stage_index.row_count
    ));

    if overview.stage_index.sample_rows.is_empty() {
        ui.label("本地数据库里还没有 PRTS 关卡定义记录。");
        return;
    }

    ui.label("示例关卡定义：");
    egui::Grid::new("prts_stage_index_preview")
        .num_columns(4)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("关卡 ID");
            ui.strong("代号");
            ui.strong("页面名");
            ui.strong("分类");
            ui.end_row();

            for row in &overview.stage_index.sample_rows {
                ui.label(row.stage_id.as_str());
                ui.label(row.code.as_str());
                ui.label(row.page_title.as_deref().unwrap_or("暂无"));
                ui.label(if row.categories.is_empty() {
                    "暂无".to_string()
                } else {
                    row.categories.join(" / ")
                });
                ui.end_row();
            }
        });
}

fn render_penguin_overview(ui: &mut egui::Ui, overview: &PenguinSyncOverview, timezone: &str) {
    ui.label(
        "当前增量策略：会先对 matrix / stages / items 执行 `HEAD` 预检查 `Last-Modified`，三者都未变化时直接跳过完整拉取。",
    );
    ui.separator();
    render_source_overview(ui, "penguin_overview", &overview.source, timezone);
    ui.separator();
    ui.label(format!("当前矩阵记录数：{}", overview.row_count));
    ui.label(format!(
        "当前可展示关卡数：{}",
        overview.current_stage_count
    ));
    ui.label("当前掉落预览会优先展示活动中掉落蓝色材料的关卡，其次展示其他关卡；各组内按最近上传量降序排序；额外物资默认折叠不展示。");

    if overview.sample_stages.is_empty() {
        ui.label("本地数据库里还没有 Penguin 掉率矩阵记录。");
        return;
    }

    ui.label("当前掉落预览：");
    egui::Grid::new("penguin_matrix_preview")
        .num_columns(4)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("关卡");
            ui.strong("体力");
            ui.strong("最近上传");
            ui.strong("当前掉落");
            ui.end_row();

            for stage in &overview.sample_stages {
                ui.label(format!("{} ({})", stage.stage_name, stage.stage_code));
                ui.label(
                    stage
                        .ap_cost
                        .map(|value| format!("{value} 理智"))
                        .unwrap_or_else(|| "暂无".to_string()),
                );
                ui.label(format!("{} 次", stage.recent_upload_count));
                ui.label(render_penguin_stage_drop_sections(stage));
                ui.end_row();
            }
        });
}

fn render_official_notice_overview(
    ui: &mut egui::Ui,
    overview: &OfficialNoticeSyncOverview,
    timezone: &str,
) {
    ui.label(
        "官方公告当前仍按全量同步：列表页没有稳定的轻量版本锚点，无法安全判断“未变化”并跳过抓取。",
    );
    ui.separator();
    render_source_overview(ui, "official_notice_overview", &overview.source, timezone);
    ui.separator();
    ui.label(format!("当前官方公告记录数：{}", overview.row_count));
    ui.label("当前 raw cache 与 `external_event_notice` 现阶段都保留官网全量公告当前态；像“是否属于会开放资源关卡的活动”这类更细语义，暂不在规则层硬筛，后续再交给更强分类器或 DeepSeek。");

    if overview.sample_rows.is_empty() {
        ui.label("本地数据库里还没有官方公告记录。");
        return;
    }

    ui.label("最近官方公告示例：");
    egui::Grid::new("official_notice_preview")
        .num_columns(6)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("类型");
            ui.strong("标题");
            ui.strong("发布时间");
            ui.strong("开始时间");
            ui.strong("结束时间");
            ui.strong("已确认");
            ui.end_row();

            for row in &overview.sample_rows {
                ui.label(row.notice_type.as_str());
                ui.label(row.title.as_str());
                ui.label(format_datetime_text(&row.published_at, timezone));
                ui.label(format_optional_datetime(&row.start_at, timezone));
                ui.label(format_optional_datetime(&row.end_at, timezone));
                ui.label(if row.confirmed { "是" } else { "否" });
                ui.end_row();
            }
        });
}

fn build_penguin_stage_displays(rows: &[PenguinDropDisplayRecord]) -> Vec<PenguinStageDisplay> {
    let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
    let mut grouped = std::collections::BTreeMap::<String, Vec<&PenguinDropDisplayRecord>>::new();

    for row in rows {
        if !is_currently_accessible_penguin_drop(row, now_ms) {
            continue;
        }

        grouped.entry(row.stage_id.clone()).or_default().push(row);
    }

    let mut stages = grouped
        .into_iter()
        .filter_map(|(stage_id, stage_rows)| {
            let first = *stage_rows.first()?;
            let stage_code = first.stage_code.clone().unwrap_or_else(|| stage_id.clone());
            let mut normal_drop_totals = std::collections::BTreeMap::<String, f64>::new();
            let mut special_drop_totals = std::collections::BTreeMap::<String, f64>::new();
            let mut recent_upload_count = 0_i64;
            let is_priority_stage = is_priority_penguin_stage(&stage_id, &stage_rows, now_ms);

            for row in &stage_rows {
                if row.item_name.trim().is_empty() || row.sample_count <= 0 {
                    continue;
                }

                recent_upload_count = recent_upload_count.max(row.sample_count);
                let per_run = row.drop_count as f64 / row.sample_count as f64;
                if per_run <= 0.0 {
                    continue;
                }

                match displayable_drop_bucket(row.drop_type.as_deref()) {
                    Some(PenguinDropBucket::Normal) => {
                        *normal_drop_totals.entry(row.item_name.clone()).or_default() += per_run;
                    }
                    Some(PenguinDropBucket::Special) => {
                        *special_drop_totals
                            .entry(row.item_name.clone())
                            .or_default() += per_run;
                    }
                    None => {}
                }
            }

            let normal_drops = build_penguin_drop_displays(first.ap_cost, normal_drop_totals);
            let special_drops = build_penguin_drop_displays(first.ap_cost, special_drop_totals);

            if normal_drops.is_empty() && special_drops.is_empty() {
                return None;
            }

            Some(PenguinStageDisplay {
                stage_name: display_penguin_stage_name(&stage_id, stage_code.as_str()),
                stage_code,
                ap_cost: first.ap_cost,
                recent_upload_count,
                is_priority_stage,
                normal_drops,
                special_drops,
            })
        })
        .collect::<Vec<_>>();

    stages.sort_by(|left, right| {
        right
            .is_priority_stage
            .cmp(&left.is_priority_stage)
            .then_with(|| right.recent_upload_count.cmp(&left.recent_upload_count))
            .then_with(|| left.stage_code.cmp(&right.stage_code))
    });
    stages
}

fn is_currently_accessible_penguin_drop(row: &PenguinDropDisplayRecord, now_ms: i128) -> bool {
    if !row.stage_exists || is_excluded_penguin_stage(row) {
        return false;
    }

    let stage_open_at = row
        .stage_open_at
        .as_deref()
        .and_then(|value| value.parse::<i128>().ok());
    let stage_close_at = row
        .stage_close_at
        .as_deref()
        .and_then(|value| value.parse::<i128>().ok());
    let stage_open_ok = stage_open_at.is_none_or(|start| start <= now_ms);
    let stage_close_ok = stage_close_at.is_none_or(|end| end >= now_ms);
    let drop_start_ok = row
        .window_start_at
        .as_deref()
        .and_then(|value| value.parse::<i128>().ok())
        .is_none_or(|start| start <= now_ms);
    let drop_end_ok = row
        .window_end_at
        .as_deref()
        .and_then(|value| value.parse::<i128>().ok())
        .is_none_or(|end| end >= now_ms);

    if requires_explicit_activity_window(row) && stage_open_at.is_none() && stage_close_at.is_none()
    {
        return false;
    }

    stage_open_ok && stage_close_ok && drop_start_ok && drop_end_ok
}

fn is_excluded_penguin_stage(row: &PenguinDropDisplayRecord) -> bool {
    row.stage_id == "recruit" || row.stage_code.as_deref() == Some("公开招募")
}

fn display_penguin_stage_name(stage_id: &str, stage_code: &str) -> String {
    if stage_id.starts_with("main_") {
        format!("主线 {stage_code}")
    } else if stage_id.starts_with("weekly_") {
        format!("资源收集 {stage_code}")
    } else if stage_id.starts_with("tough_") {
        format!("磨难 {stage_code}")
    } else if stage_id.starts_with("sub_") {
        format!("插曲 {stage_code}")
    } else if stage_id.starts_with("act") || stage_id.starts_with("side") {
        format!("活动 {stage_code}")
    } else {
        stage_code.to_string()
    }
}

fn is_priority_penguin_stage(
    stage_id: &str,
    stage_rows: &[&PenguinDropDisplayRecord],
    now_ms: i128,
) -> bool {
    let stage_type = stage_rows
        .iter()
        .find_map(|row| row.stage_type.as_deref())
        .map(str::trim);
    let is_event_stage = matches!(stage_type, Some("ACTIVITY"))
        || stage_id.starts_with("act")
        || stage_id.starts_with("side");
    let is_time_limited_event = stage_rows.iter().any(|row| {
        let close_at = row
            .stage_close_at
            .as_deref()
            .and_then(|value| value.parse::<i128>().ok());
        close_at.is_some_and(|end| end >= now_ms)
    });

    is_event_stage
        && is_time_limited_event
        && stage_rows.iter().any(|row| {
            displayable_drop_bucket(row.drop_type.as_deref()).is_some()
                && is_blue_material_drop(row.item_type.as_deref(), row.item_rarity)
        })
}

fn requires_explicit_activity_window(row: &PenguinDropDisplayRecord) -> bool {
    row.stage_type
        .as_deref()
        .map(str::trim)
        .map(|stage_type| stage_type == "ACTIVITY")
        .unwrap_or_else(|| {
            !(row.stage_id.starts_with("main_")
                || row.stage_id.starts_with("wk_")
                || row.stage_id.starts_with("weekly_")
                || row.stage_id.starts_with("sub_")
                || row.stage_id.starts_with("tough_"))
        })
}

fn is_blue_material_drop(item_type: Option<&str>, item_rarity: Option<i64>) -> bool {
    let is_material = item_type
        .map(str::trim)
        .map(|value| value == "MATERIAL" || value.contains("材料"))
        .unwrap_or(false);

    is_material && item_rarity == Some(2)
}

fn render_penguin_drop_line(drop: &PenguinDropDisplay) -> String {
    let probability_percent = drop.probability * 100.0;
    match drop.expected_ap {
        Some(expected_ap) => format!(
            "{} {:.2}% / 约 {:.1} 体力一个",
            drop.item_name, probability_percent, expected_ap
        ),
        None => format!("{} {:.2}%", drop.item_name, probability_percent),
    }
}

fn render_penguin_stage_drop_sections(stage: &PenguinStageDisplay) -> String {
    let mut sections = Vec::new();

    if !stage.normal_drops.is_empty() {
        sections.push(format!(
            "常规掉落\n{}",
            stage
                .normal_drops
                .iter()
                .map(render_penguin_drop_line)
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if !stage.special_drops.is_empty() {
        sections.push(format!(
            "特殊掉落\n{}",
            stage
                .special_drops
                .iter()
                .map(render_penguin_drop_line)
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    sections.join("\n\n")
}

fn build_penguin_drop_displays(
    ap_cost: Option<i64>,
    drop_totals: std::collections::BTreeMap<String, f64>,
) -> Vec<PenguinDropDisplay> {
    let mut drops = drop_totals
        .into_iter()
        .map(|(item_name, probability)| {
            let expected_ap = ap_cost.and_then(|cost| {
                if probability > 0.0 {
                    Some(cost as f64 / probability)
                } else {
                    None
                }
            });

            PenguinDropDisplay {
                item_name,
                probability,
                expected_ap,
            }
        })
        .collect::<Vec<_>>();

    drops.sort_by(|left, right| {
        right
            .probability
            .partial_cmp(&left.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.item_name.cmp(&right.item_name))
    });
    drops
}

fn build_prts_operator_growth_display_rows(
    rows: &[ExternalOperatorGrowthRecord],
) -> Vec<PrtsOperatorGrowthDisplayRow> {
    let mut display_rows = Vec::new();
    let mut index = 0;

    while index < rows.len() {
        let row = &rows[index];

        if row.material_slot == "通用"
            && let Some((mut min_level, mut max_level)) =
                parse_skill_upgrade_stage_range(&row.stage_label)
        {
            let operator_id = row.operator_id.clone();
            let operator_name_zh = row.operator_name_zh.clone();
            let mut material_summaries = Vec::new();

            while index < rows.len() {
                let current = &rows[index];
                if current.operator_id != operator_id || current.material_slot != "通用" {
                    break;
                }

                let Some((current_start, current_end)) =
                    parse_skill_upgrade_stage_range(&current.stage_label)
                else {
                    break;
                };

                min_level = min_level.min(current_start);
                max_level = max_level.max(current_end);
                material_summaries.push(current.material_summary.clone());
                index += 1;
            }

            display_rows.push(PrtsOperatorGrowthDisplayRow {
                operator_name_zh,
                stage_label: format!("{min_level}→{max_level}"),
                material_slot: "通用".to_string(),
                material_summary: summarize_growth_material_totals(&material_summaries),
            });
            continue;
        }

        display_rows.push(PrtsOperatorGrowthDisplayRow {
            operator_name_zh: row.operator_name_zh.clone(),
            stage_label: row.stage_label.clone(),
            material_slot: row.material_slot.clone(),
            material_summary: row.material_summary.clone(),
        });
        index += 1;
    }

    display_rows
}

fn parse_skill_upgrade_stage_range(stage_label: &str) -> Option<(i32, i32)> {
    let (start, end) = stage_label.split_once('→')?;
    let start = start.trim().parse::<i32>().ok()?;
    let end = end.trim().parse::<i32>().ok()?;
    Some((start, end))
}

fn summarize_growth_material_totals(material_summaries: &[String]) -> String {
    let mut material_order = Vec::new();
    let mut material_totals = HashMap::new();
    let mut passthrough_segments = Vec::new();

    for material_summary in material_summaries {
        for segment in material_summary.split('/') {
            let trimmed = segment.trim();
            if trimmed.is_empty() || trimmed == "暂无" {
                continue;
            }

            if let Some((name, count)) = parse_growth_material_segment(trimmed) {
                if !material_totals.contains_key(&name) {
                    material_order.push(name.clone());
                }
                *material_totals.entry(name).or_insert(0_i32) += count;
            } else {
                passthrough_segments.push(trimmed.to_string());
            }
        }
    }

    let mut parts = material_order
        .into_iter()
        .filter_map(|name| {
            material_totals
                .get(&name)
                .copied()
                .map(|count| format!("{name} x{count}"))
        })
        .collect::<Vec<_>>();
    parts.extend(passthrough_segments);

    if parts.is_empty() {
        "暂无".to_string()
    } else {
        parts.join(" / ")
    }
}

fn parse_growth_material_segment(segment: &str) -> Option<(String, i32)> {
    let (name, count) = segment.rsplit_once(" x")?;
    let count = count.trim().parse::<i32>().ok()?;
    Some((name.trim().to_string(), count))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PenguinDropBucket {
    Normal,
    Special,
}

fn displayable_drop_bucket(drop_type: Option<&str>) -> Option<PenguinDropBucket> {
    match drop_type.map(str::trim) {
        Some("SPECIAL_DROP") => Some(PenguinDropBucket::Special),
        Some("EXTRA_DROP") | Some("FURNITURE") => None,
        _ => Some(PenguinDropBucket::Normal),
    }
}

fn overview_row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.label(label);
    ui.label(value.into());
    ui.end_row();
}

fn optional_text(value: &Option<String>) -> String {
    value.clone().unwrap_or_else(|| "暂无".to_string())
}

fn format_optional_datetime(value: &Option<String>, timezone: &str) -> String {
    value
        .as_deref()
        .map(|value| format_datetime_text(value, timezone))
        .unwrap_or_else(|| "暂无".to_string())
}

fn format_datetime_text(value: &str, timezone: &str) -> String {
    let Some(offset) = configured_offset(timezone) else {
        return value.to_string();
    };

    if let Ok(datetime) = OffsetDateTime::parse(value, &Rfc3339) {
        return format_with_offset(datetime.to_offset(offset), offset);
    }

    if let Ok(epoch_millis) = value.parse::<i128>() {
        let seconds = epoch_millis.div_euclid(1_000);
        let milliseconds = epoch_millis.rem_euclid(1_000) as i64;
        if let Ok(seconds) = i64::try_from(seconds)
            && let Ok(datetime) = OffsetDateTime::from_unix_timestamp(seconds)
        {
            return format_with_offset(
                datetime + time::Duration::milliseconds(milliseconds),
                offset,
            );
        }
    }

    value.to_string()
}

fn configured_offset(timezone: &str) -> Option<UtcOffset> {
    match timezone.trim() {
        "Asia/Shanghai" => UtcOffset::from_hms(8, 0, 0).ok(),
        "UTC" | "Etc/UTC" | "Z" => Some(UtcOffset::UTC),
        value => parse_fixed_offset(value),
    }
}

fn parse_fixed_offset(value: &str) -> Option<UtcOffset> {
    let trimmed = value.trim();
    if trimmed.len() != 6 || !(trimmed.starts_with('+') || trimmed.starts_with('-')) {
        return None;
    }

    let sign = if trimmed.starts_with('-') { -1 } else { 1 };
    let hours = trimmed[1..3].parse::<i8>().ok()?;
    let minutes = trimmed[4..6].parse::<i8>().ok()?;
    if &trimmed[3..4] != ":" {
        return None;
    }

    UtcOffset::from_hms(sign * hours, sign * minutes, 0).ok()
}

fn format_with_offset(datetime: OffsetDateTime, offset: UtcOffset) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC{:+03}:{:02}",
        datetime.year(),
        u8::from(datetime.month()),
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        offset.whole_hours(),
        offset.minutes_past_hour().unsigned_abs()
    )
}

fn optional_number(value: Option<i64>, suffix: &str) -> String {
    value
        .map(|value| format!("{value} {suffix}"))
        .unwrap_or_else(|| "暂无".to_string())
}

fn optional_integer(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "暂无".to_string())
}

fn bool_text(value: bool) -> &'static str {
    if value { "是" } else { "否" }
}

fn describe_config_source_zh(source: &ConfigSource) -> String {
    match source {
        ConfigSource::Defaults { expected_path } => {
            format!("默认值（未在 {} 找到配置文件）", expected_path.display())
        }
        ConfigSource::File(path) => format!("文件 {}", path.display()),
    }
}

struct SaveOutcome {
    path: PathBuf,
    logging_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigForm {
    adb_executable: String,
    game_timezone: String,
    log_directory: String,
    log_file_name: String,
    export_artifacts: bool,
    export_directory: String,
}

impl ConfigForm {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            adb_executable: config
                .adb
                .executable
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            game_timezone: config.game.timezone.clone(),
            log_directory: config.logging.directory.display().to_string(),
            log_file_name: config.logging.file_name.clone(),
            export_artifacts: config.debug.export_artifacts,
            export_directory: config.debug.export_directory.display().to_string(),
        }
    }

    fn to_config(&self) -> AppConfig {
        AppConfig {
            adb: AdbConfig {
                executable: non_empty_path(&self.adb_executable),
            },
            game: GameConfig {
                timezone: self.game_timezone.clone(),
            },
            logging: LoggingConfig {
                directory: PathBuf::from(self.log_directory.trim()),
                file_name: self.log_file_name.clone(),
            },
            debug: DebugConfig {
                export_artifacts: self.export_artifacts,
                export_directory: PathBuf::from(self.export_directory.trim()),
            },
        }
    }

    fn resolved_log_path(&self, base_directory: &Path) -> PathBuf {
        self.to_config().logging.resolved_file_path(base_directory)
    }

    fn resolved_debug_directory(&self, base_directory: &Path) -> PathBuf {
        self.to_config()
            .debug
            .resolved_export_directory(base_directory)
    }
}

fn non_empty_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn default_templates_root_for_config(page_config_path: &Path) -> PathBuf {
    let Some(parent) = page_config_path.parent() else {
        return PathBuf::from(".");
    };

    let parent_name = parent
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if parent_name.as_deref() == Some("pages") {
        parent.parent().unwrap_or(parent).to_path_buf()
    } else {
        parent.to_path_buf()
    }
}

fn discover_default_vision_templates_root(working_directory: &Path) -> PathBuf {
    let fallback = working_directory.join("assets").join("templates");
    let search_roots = default_vision_template_search_roots(working_directory);
    find_vision_templates_root_from_search_roots(&search_roots).unwrap_or(fallback)
}

fn default_vision_template_search_roots(working_directory: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    push_unique_path(&mut roots, working_directory.to_path_buf());

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        push_unique_path(&mut roots, parent.to_path_buf());
    }

    push_unique_path(&mut roots, PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    roots
}

fn find_vision_templates_root_from_search_roots(search_roots: &[PathBuf]) -> Option<PathBuf> {
    let mut seen = HashSet::new();

    for search_root in search_roots {
        for ancestor in search_root.ancestors() {
            let candidate = ancestor.join("assets").join("templates");
            if !seen.insert(candidate.clone()) {
                continue;
            }

            if candidate.join("pages").is_dir() {
                return Some(candidate);
            }
        }
    }

    None
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn discover_vision_page_presets(templates_root: &Path) -> Result<Vec<VisionPagePreset>, String> {
    let pages_root = templates_root.join("pages");
    if !pages_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut config_paths = Vec::new();
    collect_files_with_extension(&pages_root, "json", &mut config_paths)?;

    let mut presets = Vec::new();
    for config_path in config_paths {
        let catalog = load_page_state_catalog_from_path(&config_path)
            .map_err(|error| format!("读取页面配置 {} 失败：{error}", config_path.display()))?;
        let templates_root = default_templates_root_for_config(&config_path);

        for page in catalog.pages {
            presets.push(VisionPagePreset {
                key: format!("{}::{}", config_path.display(), page.page_id),
                page_id: page.page_id.clone(),
                display_name: page.display_name,
                config_path: config_path.clone(),
                templates_root: templates_root.clone(),
                marker_count: page.confirmation_markers.len(),
                roi_count: page.rois.len(),
            });
        }
    }

    presets.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.page_id.cmp(&right.page_id))
            .then_with(|| left.config_path.cmp(&right.config_path))
    });

    Ok(presets)
}

fn collect_files_with_extension(
    directory: &Path,
    extension: &str,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("读取目录 {} 失败：{error}", directory.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("读取目录项 {} 失败：{error}", directory.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_extension(&path, extension, files)?;
            continue;
        }

        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            files.push(path);
        }
    }

    files.sort();
    Ok(())
}

fn default_vision_output_dir(base_directory: &Path, page_id: &str) -> PathBuf {
    let page_slug = if page_id.trim().is_empty() {
        "unnamed-page".to_string()
    } else {
        sanitize_debug_file_name(page_id.trim())
    };

    base_directory
        .join("debug-artifacts")
        .join("vision-inspect")
        .join(page_slug)
}

fn sanitize_debug_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "roi".to_string()
    } else {
        sanitized
    }
}

fn describe_confirmation_strategy(
    strategy: akbox_device::PageConfirmationStrategy,
) -> &'static str {
    match strategy {
        akbox_device::PageConfirmationStrategy::TemplateFingerprint => "模板指纹",
        akbox_device::PageConfirmationStrategy::TextHint => "文本提示",
        akbox_device::PageConfirmationStrategy::IconTemplate => "图标模板",
        akbox_device::PageConfirmationStrategy::DominantColor => "主色块",
    }
}

fn describe_roi_purpose(purpose: RoiPurpose) -> &'static str {
    match purpose {
        RoiPurpose::PageAnchor => "页面锚点",
        RoiPurpose::NumericOcr => "数字 OCR",
        RoiPurpose::ShortTextOcr => "短文本 OCR",
        RoiPurpose::IconTemplate => "图标模板",
        RoiPurpose::ColorFingerprint => "颜色指纹",
        RoiPurpose::Generic => "通用",
    }
}

fn optional_score(value: Option<f32>) -> String {
    value
        .map(|score| format!("{score:.4}"))
        .unwrap_or_else(|| "-".to_string())
}

#[derive(Clone)]
struct UiNotice {
    kind: UiNoticeKind,
    message: String,
}

impl UiNotice {
    fn info(message: impl Into<String>) -> Self {
        Self {
            kind: UiNoticeKind::Info,
            message: message.into(),
        }
    }

    fn success(message: impl Into<String>) -> Self {
        Self {
            kind: UiNoticeKind::Success,
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            kind: UiNoticeKind::Warning,
            message: message.into(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            kind: UiNoticeKind::Error,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy)]
enum UiNoticeKind {
    Info,
    Success,
    Warning,
    Error,
}

fn render_notice(ui: &mut egui::Ui, notice: &UiNotice) {
    let color = match notice.kind {
        UiNoticeKind::Info => egui::Color32::from_rgb(54, 102, 170),
        UiNoticeKind::Success => egui::Color32::from_rgb(48, 120, 64),
        UiNoticeKind::Warning => egui::Color32::from_rgb(140, 108, 32),
        UiNoticeKind::Error => egui::Color32::from_rgb(132, 52, 52),
    };

    egui::Frame::default()
        .fill(color.gamma_multiply(0.18))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.colored_label(color, notice.message.as_str());
        });
}

#[cfg(test)]
mod tests {
    use super::ConfigForm;
    use super::RunningSklandProfileTask;
    use super::SettingsPageState;
    use super::SklandApiEnvelope;
    use super::SklandGenerateCredData;
    use super::SklandLoginPageState;
    use super::SklandPlayerInfoInspectOutcome;
    use super::SklandProfileTaskFinished;
    use super::SklandProfileTaskKind;
    use super::SklandResolvedAuth;
    use super::SyncPageState;
    use super::VisionDebugPageState;
    use super::VisionInputSourceMode;
    use super::build_penguin_stage_displays;
    use super::build_prts_operator_growth_display_rows;
    use super::default_skland_auth_file_path;
    use super::default_skland_uid_file_value;
    use super::default_templates_root_for_config;
    use super::default_vision_output_dir;
    use super::discover_vision_page_presets;
    use super::find_vision_templates_root_from_search_roots;
    use super::read_skland_auth_file_or_default;
    use super::summarize_skland_response_body;
    use super::write_skland_auth_file;
    use akbox_core::config::AppConfig;
    use akbox_core::config::ConfigSource;
    use akbox_core::config::LoadedConfig;
    use akbox_data::ExternalOperatorGrowthRecord;
    use akbox_data::PenguinDropDisplayRecord;
    use akbox_data::default_database_path;
    use eframe::egui;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use time::OffsetDateTime;

    #[test]
    fn config_form_round_trip_preserves_values() {
        let config = AppConfig::default();

        let rebuilt = ConfigForm::from_config(&config).to_config();

        assert_eq!(rebuilt, config);
    }

    #[test]
    fn settings_state_detects_unsaved_changes() {
        let loaded = LoadedConfig {
            source: ConfigSource::Defaults {
                expected_path: PathBuf::from("ArkAgent.toml"),
            },
            config: AppConfig::default(),
        };
        let mut state = SettingsPageState::from_loaded(&loaded);

        assert!(!state.is_dirty());
        state.form.game_timezone = "UTC".to_string();
        assert!(state.is_dirty());
    }

    #[test]
    fn sync_page_uses_default_database_path() {
        let base_directory = unique_test_path("desktop-sync");
        fs::create_dir_all(&base_directory).unwrap();

        let state = SyncPageState::new(&base_directory);

        assert_eq!(
            PathBuf::from(state.database_path_text),
            default_database_path(&base_directory)
        );

        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn default_skland_auth_file_path_uses_working_directory() {
        let base_directory = PathBuf::from(r"C:\repo");

        let path = default_skland_auth_file_path(&base_directory);

        assert_eq!(path, PathBuf::from(r"C:\repo\skland-auth.local.toml"));
    }

    #[test]
    fn skland_auth_file_round_trip_preserves_uid_file_and_updates_auth_fields() {
        let base_directory = unique_test_path("desktop-skland-auth");
        let auth_file_path = base_directory.join("local").join("skland-auth.local.toml");
        fs::create_dir_all(&base_directory).unwrap();

        let auth = SklandResolvedAuth {
            access_token: "access-token-value".to_string(),
            cred: "cred-value".to_string(),
            token: "signing-token-value".to_string(),
            user_id: Some("123456".to_string()),
        };

        write_skland_auth_file(&auth_file_path, "uid.txt", &auth).unwrap();

        let loaded = read_skland_auth_file_or_default(&auth_file_path).unwrap();

        assert!(loaded.exists);
        assert_eq!(loaded.file.skland.uid_file, "uid.txt");
        assert_eq!(loaded.file.skland.access_token, "access-token-value");
        assert_eq!(loaded.file.skland.cred, "cred-value");
        assert_eq!(loaded.file.skland.token, "signing-token-value");
        assert_eq!(loaded.file.skland.user_id, "123456");
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn read_skland_auth_file_or_default_uses_default_uid_file_when_missing() {
        let base_directory = unique_test_path("desktop-skland-auth-missing");
        fs::create_dir_all(&base_directory).unwrap();
        let auth_file_path = base_directory.join("skland-auth.local.toml");

        let loaded = read_skland_auth_file_or_default(&auth_file_path).unwrap();

        assert!(!loaded.exists);
        assert_eq!(loaded.file.skland.uid_file, default_skland_uid_file_value());
        assert!(loaded.file.skland.access_token.is_empty());
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn skland_profile_task_is_polled_without_login_task() {
        let base_directory = unique_test_path("desktop-skland-profile-poll");
        fs::create_dir_all(&base_directory).unwrap();
        let mut state = SklandLoginPageState::new(&base_directory);
        state.running_profile_task = Some(RunningSklandProfileTask {
            kind: SklandProfileTaskKind::InspectPlayerInfo,
            handle: std::thread::spawn(|| {
                SklandProfileTaskFinished::Inspect(Ok(SklandPlayerInfoInspectOutcome {
                    source_id: "skland.player-info.current".to_string(),
                    cache_key: "skland:player-info:current".to_string(),
                    revision: "test-revision".to_string(),
                    cache_size_bytes: 123,
                    uid: "local-test-uid".to_string(),
                    account_name: Some("测试博士".to_string()),
                    status_store_ts: Some(1710600000),
                    status_keys: vec!["name".to_string(), "storeTs".to_string()],
                    binding_count: 1,
                    char_count: 2,
                    assist_count: 1,
                    equipment_info_count: 3,
                    char_info_count: 4,
                    has_building: true,
                    building_keys: vec!["rooms".to_string()],
                    has_control: false,
                    has_meeting: false,
                    has_training: false,
                    has_hire: false,
                    dormitory_count: 0,
                    manufacture_count: 0,
                    trading_count: 0,
                    power_count: 0,
                    tired_char_count: 0,
                    sample_operator: None,
                }))
            }),
        });

        state.poll_running_task(&egui::Context::default(), &base_directory);

        assert!(state.running_profile_task.is_none());
        assert_eq!(state.status_text, "森空岛 player/info 检查完成");
        assert_eq!(
            state
                .last_player_info_inspect
                .as_ref()
                .map(|value| value.revision.as_str()),
            Some("test-revision")
        );

        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn skland_envelope_deserializes_status_shape() {
        let envelope = serde_json::from_str::<SklandApiEnvelope<serde_json::Value>>(
            r#"{"status":0,"msg":"ok","data":{"scanId":"abc"}}"#,
        )
        .unwrap();

        assert_eq!(envelope.status_code(), Some(0));
        assert_eq!(envelope.message_text(), "ok");
        assert_eq!(
            envelope
                .data
                .unwrap()
                .get("scanId")
                .and_then(|value| value.as_str()),
            Some("abc")
        );
    }

    #[test]
    fn skland_envelope_deserializes_code_shape() {
        let envelope = serde_json::from_str::<SklandApiEnvelope<SklandGenerateCredData>>(
            r#"{"code":0,"message":"OK","data":{"cred":"cred-value","token":"token-value","userId":"42"}}"#,
        )
        .unwrap();

        assert_eq!(envelope.status_code(), Some(0));
        assert_eq!(envelope.message_text(), "OK");
        let data = envelope.data.unwrap();
        assert_eq!(data.cred, "cred-value");
        assert_eq!(data.token, "token-value");
        assert_eq!(data.user_id.as_deref(), Some("42"));
    }

    #[test]
    fn summarize_skland_response_body_only_exposes_non_sensitive_shape() {
        let summary = summarize_skland_response_body(
            r#"{"code":0,"message":"OK","data":{"cred":"secret-cred","token":"secret-token","userId":"42"}}"#,
        );

        assert!(summary.contains("code=0"));
        assert!(summary.contains("message=OK"));
        assert!(summary.contains("data_keys=cred,token,userId"));
        assert!(!summary.contains("secret-cred"));
        assert!(!summary.contains("secret-token"));
    }

    #[test]
    fn default_templates_root_uses_templates_parent_for_pages_directory() {
        let path = PathBuf::from(r"C:\repo\assets\templates\pages\inventory_main.json");

        let root = default_templates_root_for_config(&path);

        assert_eq!(root, PathBuf::from(r"C:\repo\assets\templates"));
    }

    #[test]
    fn default_vision_output_dir_uses_sanitized_page_id() {
        let base_directory = PathBuf::from(r"C:\repo");

        let output_dir = default_vision_output_dir(&base_directory, "inventory/main");

        assert_eq!(
            output_dir,
            PathBuf::from(r"C:\repo\debug-artifacts\vision-inspect\inventory_main")
        );
    }

    #[test]
    fn discover_vision_page_presets_flattens_catalog_entries() {
        let base_directory = unique_test_path("desktop-vision-pages");
        let pages_directory = base_directory.join("pages");
        fs::create_dir_all(&pages_directory).unwrap();
        let config_path = pages_directory.join("inventory_main.json");
        fs::write(
            &config_path,
            r#"{
  "pages": [
    {
      "page_id": "inventory_main",
      "display_name": "仓库主页",
      "reference_resolution": { "width": 1280, "height": 720 },
      "confirmation_markers": [],
      "rois": []
    },
    {
      "page_id": "operator_list",
      "display_name": "干员列表",
      "reference_resolution": { "width": 1280, "height": 720 },
      "confirmation_markers": [],
      "rois": [
        {
          "roi_id": "anchor",
          "display_name": "Anchor",
          "rect": { "x": 0, "y": 0, "width": 100, "height": 50 },
          "purpose": "page_anchor",
          "preprocess_steps": [],
          "confidence_threshold": 0.9,
          "low_confidence_policy": "queue_review"
        }
      ]
    }
  ]
}"#,
        )
        .unwrap();

        let pages = discover_vision_page_presets(&base_directory).unwrap();

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].display_name, "仓库主页");
        assert_eq!(pages[1].page_id, "operator_list");
        assert_eq!(pages[1].roi_count, 1);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn find_vision_templates_root_walks_up_from_nested_start_directory() {
        let base_directory = unique_test_path("desktop-vision-root-discovery");
        let nested_directory = base_directory.join("dist").join("release");
        let templates_pages_directory = base_directory
            .join("assets")
            .join("templates")
            .join("pages");
        fs::create_dir_all(&nested_directory).unwrap();
        fs::create_dir_all(&templates_pages_directory).unwrap();

        let root = find_vision_templates_root_from_search_roots(&[nested_directory]).unwrap();

        assert_eq!(root, base_directory.join("assets").join("templates"));
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn vision_state_build_request_rejects_missing_page_id() {
        let base_directory = unique_test_path("desktop-vision");
        fs::create_dir_all(&base_directory).unwrap();
        let mut state = VisionDebugPageState::new(&base_directory);
        state.input_source_mode = VisionInputSourceMode::LocalPng;
        state.page_config_path_text = base_directory.join("page.json").display().to_string();
        state.page_id_text.clear();
        state.input_png_path_text = base_directory.join("sample.png").display().to_string();

        let error = state.build_request(&base_directory, None).unwrap_err();

        assert!(error.contains("页面 ID 不能为空"));
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn prts_operator_growth_preview_merges_generic_skill_levels_into_one_to_seven() {
        let rows = vec![
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:skill_1_2:global".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "1→2".to_string(),
                material_slot: "通用".to_string(),
                material_summary: "技巧概要·卷1 x5".to_string(),
            },
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:skill_2_3:global".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "2→3".to_string(),
                material_slot: "通用".to_string(),
                material_summary: "技巧概要·卷1 x5 / 破损装置 x4 / 酯原料 x4".to_string(),
            },
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:skill_3_4:global".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "3→4".to_string(),
                material_slot: "通用".to_string(),
                material_summary: "技巧概要·卷2 x8 / 固源岩 x7".to_string(),
            },
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:skill_4_5:global".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "4→5".to_string(),
                material_slot: "通用".to_string(),
                material_summary: "技巧概要·卷2 x8 / 糖 x4 / 酮凝集 x4".to_string(),
            },
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:skill_5_6:global".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "5→6".to_string(),
                material_slot: "通用".to_string(),
                material_summary: "技巧概要·卷3 x8 / 异铁 x4".to_string(),
            },
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:skill_6_7:global".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "6→7".to_string(),
                material_slot: "通用".to_string(),
                material_summary: "技巧概要·卷3 x8 / 异铁组 x4".to_string(),
            },
            ExternalOperatorGrowthRecord {
                growth_id: "char_103_angel:mastery_1_1:skill_1".to_string(),
                operator_id: "char_103_angel".to_string(),
                operator_name_zh: "能天使".to_string(),
                stage_label: "等级1".to_string(),
                material_slot: "第1技能".to_string(),
                material_summary: "技巧概要·卷3 x6".to_string(),
            },
        ];

        let display_rows = build_prts_operator_growth_display_rows(&rows);

        assert_eq!(display_rows.len(), 2);
        assert_eq!(display_rows[0].operator_name_zh, "能天使");
        assert_eq!(display_rows[0].stage_label, "1→7");
        assert_eq!(display_rows[0].material_slot, "通用");
        assert_eq!(
            display_rows[0].material_summary,
            "技巧概要·卷1 x10 / 破损装置 x4 / 酯原料 x4 / 技巧概要·卷2 x16 / 固源岩 x7 / 糖 x4 / 酮凝集 x4 / 技巧概要·卷3 x16 / 异铁 x4 / 异铁组 x4"
        );
        assert!(!display_rows[0].material_summary.contains("1→2："));
        assert_eq!(display_rows[1].stage_label, "等级1");
        assert_eq!(display_rows[1].material_slot, "第1技能");
    }

    #[test]
    fn penguin_preview_prioritizes_event_blue_material_stages_before_other_hot_stages() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let open_at = (now_ms - 60_000).to_string();
        let close_at = (now_ms + 60_000).to_string();
        let rows = vec![
            PenguinDropDisplayRecord {
                stage_id: "act35side_01".to_string(),
                stage_code: Some("HE-6".to_string()),
                stage_type: Some("ACTIVITY".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: Some(open_at.clone()),
                stage_close_at: Some(close_at.clone()),
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30043".to_string(),
                item_name: "转质盐组".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 180,
                drop_count: 64,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "main_10-6".to_string(),
                stage_code: Some("10-6".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30033".to_string(),
                item_name: "轻锰矿组".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 320,
                drop_count: 90,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "side_17-7".to_string(),
                stage_code: Some("DV-8".to_string()),
                stage_type: Some("ACTIVITY".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: Some(open_at),
                stage_close_at: Some(close_at),
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30012".to_string(),
                item_name: "固源岩".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(1),
                sample_count: 260,
                drop_count: 110,
                window_start_at: None,
                window_end_at: None,
            },
        ];

        let stages = build_penguin_stage_displays(&rows);

        assert_eq!(stages.len(), 3);
        assert_eq!(stages[0].stage_code, "HE-6");
        assert!(stages[0].is_priority_stage);
        assert_eq!(stages[1].stage_code, "10-6");
        assert_eq!(stages[1].recent_upload_count, 320);
        assert_eq!(stages[2].stage_code, "DV-8");
    }

    #[test]
    fn penguin_preview_separates_special_drops_and_hides_extra_drops() {
        let rows = vec![
            PenguinDropDisplayRecord {
                stage_id: "main_01-07".to_string(),
                stage_code: Some("1-7".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(6),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30012".to_string(),
                item_name: "固源岩".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(1),
                sample_count: 300,
                drop_count: 150,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "main_01-07".to_string(),
                stage_code: Some("1-7".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(6),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("SPECIAL_DROP".to_string()),
                item_id: "31043".to_string(),
                item_name: "技巧概要·卷2".to_string(),
                item_type: Some("MATERIAL".to_string()),
                item_rarity: Some(2),
                sample_count: 300,
                drop_count: 24,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "main_01-07".to_string(),
                stage_code: Some("1-7".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(6),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("EXTRA_DROP".to_string()),
                item_id: "30011".to_string(),
                item_name: "源岩".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(0),
                sample_count: 300,
                drop_count: 80,
                window_start_at: None,
                window_end_at: None,
            },
        ];

        let stages = build_penguin_stage_displays(&rows);

        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].normal_drops.len(), 1);
        assert_eq!(stages[0].normal_drops[0].item_name, "固源岩");
        assert_eq!(stages[0].special_drops.len(), 1);
        assert_eq!(stages[0].special_drops[0].item_name, "技巧概要·卷2");
    }

    #[test]
    fn penguin_preview_hides_closed_event_stages_from_current_overview() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let closed_at = (now_ms - 60_000).to_string();

        let rows = vec![
            PenguinDropDisplayRecord {
                stage_id: "act_old_08".to_string(),
                stage_code: Some("OD-8".to_string()),
                stage_type: Some("ACTIVITY".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: Some(closed_at),
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30043".to_string(),
                item_name: "转质盐组".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 500,
                drop_count: 120,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "main_10-6".to_string(),
                stage_code: Some("10-6".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30033".to_string(),
                item_name: "轻锰矿组".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 320,
                drop_count: 90,
                window_start_at: None,
                window_end_at: None,
            },
        ];

        let stages = build_penguin_stage_displays(&rows);

        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].stage_code, "10-6");
        assert!(!stages[0].is_priority_stage);
    }

    #[test]
    fn penguin_preview_hides_activity_like_stages_without_window_metadata() {
        let rows = vec![
            PenguinDropDisplayRecord {
                stage_id: "a001_05".to_string(),
                stage_code: Some("GT-5".to_string()),
                stage_type: Some("ACTIVITY".to_string()),
                ap_cost: Some(15),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30063".to_string(),
                item_name: "扭转醇".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 171420,
                drop_count: 75788,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "main_10-6".to_string(),
                stage_code: Some("10-6".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30033".to_string(),
                item_name: "轻锰矿组".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 320,
                drop_count: 90,
                window_start_at: None,
                window_end_at: None,
            },
        ];

        let stages = build_penguin_stage_displays(&rows);

        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].stage_code, "10-6");
    }

    #[test]
    fn penguin_preview_treats_perm_activity_stages_as_non_priority_accessible_stages() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let open_at = (now_ms - 60_000).to_string();
        let close_at = (now_ms + 60_000).to_string();

        let rows = vec![
            PenguinDropDisplayRecord {
                stage_id: "act20mini_08".to_string(),
                stage_code: Some("CG-8".to_string()),
                stage_type: Some("ACTIVITY".to_string()),
                ap_cost: Some(21),
                stage_exists: true,
                stage_open_at: Some(open_at.clone()),
                stage_close_at: Some(close_at),
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30073".to_string(),
                item_name: "化合切削液".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 48025,
                drop_count: 22510,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "a001_05_perm".to_string(),
                stage_code: Some("GT-5".to_string()),
                stage_type: Some("ACTIVITY".to_string()),
                ap_cost: Some(15),
                stage_exists: true,
                stage_open_at: Some(open_at),
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30063".to_string(),
                item_name: "扭转醇".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(2),
                sample_count: 171420,
                drop_count: 75788,
                window_start_at: None,
                window_end_at: None,
            },
        ];

        let stages = build_penguin_stage_displays(&rows);

        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0].stage_code, "CG-8");
        assert!(stages[0].is_priority_stage);
        assert_eq!(stages[1].stage_code, "GT-5");
        assert!(!stages[1].is_priority_stage);
    }

    #[test]
    fn penguin_preview_hides_recruit_pseudo_stage() {
        let rows = vec![
            PenguinDropDisplayRecord {
                stage_id: "recruit".to_string(),
                stage_code: Some("公开招募".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(99),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "recruit_tag_top_operator".to_string(),
                item_name: "高级资深干员".to_string(),
                item_type: Some("unknown".to_string()),
                item_rarity: None,
                sample_count: 982_438_935,
                drop_count: 12_345,
                window_start_at: None,
                window_end_at: None,
            },
            PenguinDropDisplayRecord {
                stage_id: "main_01-07".to_string(),
                stage_code: Some("1-7".to_string()),
                stage_type: Some("MAIN".to_string()),
                ap_cost: Some(6),
                stage_exists: true,
                stage_open_at: None,
                stage_close_at: None,
                drop_type: Some("NORMAL_DROP".to_string()),
                item_id: "30012".to_string(),
                item_name: "固源岩".to_string(),
                item_type: Some("养成材料".to_string()),
                item_rarity: Some(1),
                sample_count: 1000,
                drop_count: 520,
                window_start_at: None,
                window_end_at: None,
            },
        ];

        let stages = build_penguin_stage_displays(&rows);

        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].stage_code, "1-7");
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-desktop-{label}-{}-{nanos}",
            std::process::id()
        ))
    }
}
