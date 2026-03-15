use std::path::{Path, PathBuf};
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
use akbox_data::ExternalDropMatrixRecord;
use akbox_data::PENGUIN_MATRIX_CACHE_KEY;
use akbox_data::PENGUIN_MATRIX_SOURCE_ID;
use akbox_data::PRTS_SITEINFO_CACHE_KEY;
use akbox_data::PRTS_SITEINFO_SOURCE_ID;
use akbox_data::PenguinClient;
use akbox_data::PrtsClient;
use akbox_data::RawSourceCacheSummary;
use akbox_data::SyncPenguinMatrixOutcome;
use akbox_data::SyncPrtsSiteInfoOutcome;
use akbox_data::SyncSourceStateRecord;
use akbox_data::default_database_path;
use akbox_data::sync_penguin_matrix;
use akbox_data::sync_prts_site_info;
use akbox_device::ScreenshotCaptureRequest;
use akbox_device::capture_device_screenshot_png;
use eframe::egui;

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
    sync: SyncPageState,
}

impl ArkAgentDesktopApp {
    fn new(bootstrap: DesktopBootstrap) -> Self {
        Self {
            current_page: Page::Dashboard,
            working_directory: bootstrap.working_directory.clone(),
            active_log_file: bootstrap.active_log_file,
            startup_notices: bootstrap.startup_notices,
            settings: SettingsPageState::from_loaded(&bootstrap.loaded_config),
            sync: SyncPageState::new(&bootstrap.working_directory),
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

        let request = ScreenshotCaptureRequest {
            adb_executable: config.adb.executable.clone(),
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
        ui.label("当前已有两个可操作入口：本地配置设置，以及外部数据同步总览。");
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
        ui.label(format!("PRTS 状态：{}", self.sync.prts.status_label()));
        ui.label(format!(
            "Penguin 状态：{}",
            self.sync.penguin.source.status_label()
        ));
        ui.label("截图来源：MuMu / ADB 设备截图（阶段 4 接入后可用）");
        ui.separator();

        if ui.button("打开同步页").clicked() {
            self.current_page = Page::Sync;
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
        ui.label("这里展示 PRTS 与 Penguin 的本地同步状态，并允许触发后台同步。");
        ui.separator();

        let running = self.sync.is_running();
        ui.horizontal(|ui| {
            ui.label("数据库路径");
            ui.text_edit_singleline(&mut self.sync.database_path_text);
        });

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
            ui.selectable_value(&mut self.sync.selected_tab, SyncTab::Penguin, "Penguin");
        });
        ui.separator();

        match self.sync.selected_tab {
            SyncTab::Prts => render_source_overview(ui, "prts_overview", &self.sync.prts),
            SyncTab::Penguin => render_penguin_overview(ui, &self.sync.penguin),
        }
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
        if self.sync.is_running() {
            context.request_repaint_after(Duration::from_millis(200));
        }

        egui::TopBottomPanel::top("top_bar").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("方舟看号台");
                ui.label("本地同步与配置壳层");
            });
        });

        egui::SidePanel::left("navigation")
            .resizable(false)
            .default_width(180.0)
            .show(context, |ui| {
                ui.heading("导航");
                ui.separator();
                ui.selectable_value(&mut self.current_page, Page::Dashboard, "仪表盘");
                ui.selectable_value(&mut self.current_page, Page::Sync, "同步");
                ui.selectable_value(&mut self.current_page, Page::Settings, "设置");
            });

        egui::CentralPanel::default().show(context, |ui| {
            let mut has_notice = false;

            for notice in &self.startup_notices {
                render_notice(ui, notice);
                has_notice = true;
            }

            if let Some(notice) = self.sync.notice.as_ref() {
                render_notice(ui, notice);
                has_notice = true;
            }

            if let Some(notice) = self.settings.notice.as_ref() {
                render_notice(ui, notice);
                has_notice = true;
            }

            if has_notice {
                ui.separator();
            }

            match self.current_page {
                Page::Dashboard => self.render_dashboard(ui),
                Page::Sync => self.render_sync(ui),
                Page::Settings => self.render_settings(ui),
            }
        });
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Page {
    Dashboard,
    Sync,
    Settings,
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
    selected_tab: SyncTab,
    prts: SourceSyncOverview,
    penguin: PenguinSyncOverview,
    notice: Option<UiNotice>,
    running_task: Option<RunningSyncTask>,
}

impl SyncPageState {
    fn new(working_directory: &Path) -> Self {
        let database_path = default_database_path(working_directory);
        let mut state = Self {
            database_path_text: database_path.display().to_string(),
            selected_tab: SyncTab::Prts,
            prts: SourceSyncOverview::empty(PRTS_SITEINFO_SOURCE_ID, PRTS_SITEINFO_CACHE_KEY),
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
        self.running_task.as_ref().map(|task| task.kind.label())
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

        self.notice = Some(UiNotice::info("PRTS 同步已开始，正在后台执行"));
        self.selected_tab = SyncTab::Prts;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::Prts,
            handle: thread::spawn(move || {
                SyncTaskFinished::Prts(run_prts_sync_task(&database_path, &working_directory))
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

        self.notice = Some(UiNotice::info("Penguin 同步已开始，正在后台执行"));
        self.selected_tab = SyncTab::Penguin;
        self.running_task = Some(RunningSyncTask {
            kind: SyncTaskKind::Penguin,
            handle: thread::spawn(move || {
                SyncTaskFinished::Penguin(run_penguin_sync_task(&database_path))
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
            Ok(SyncTaskFinished::Prts(result)) => match result {
                Ok(outcome) => {
                    tracing::info!(
                        source_id = %outcome.source_id,
                        revision = %outcome.revision,
                        "desktop prts sync completed"
                    );
                    self.notice = Some(build_sync_success_notice(
                        "PRTS",
                        format!(
                            "版本锚点：{}，缓存 {} 字节。",
                            outcome.revision, outcome.cache_size_bytes
                        ),
                        refresh_result,
                    ));
                }
                Err(error) => {
                    tracing::error!(%error, "desktop prts sync failed");
                    self.notice = Some(build_sync_error_notice("PRTS", error, refresh_result));
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
                            "写入 {} 条矩阵记录，版本锚点：{}。",
                            outcome.row_count, outcome.revision
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
    Penguin,
}

struct RunningSyncTask {
    kind: SyncTaskKind,
    handle: JoinHandle<SyncTaskFinished>,
}

#[derive(Copy, Clone)]
enum SyncTaskKind {
    Prts,
    Penguin,
}

impl SyncTaskKind {
    fn label(self) -> &'static str {
        match self {
            Self::Prts => "后台任务：PRTS 同步中",
            Self::Penguin => "后台任务：Penguin 同步中",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Prts => "PRTS 同步",
            Self::Penguin => "Penguin 同步",
        }
    }
}

enum SyncTaskFinished {
    Prts(Result<SyncPrtsSiteInfoOutcome, String>),
    Penguin(Result<SyncPenguinMatrixOutcome, String>),
}

#[derive(Clone)]
struct SourceSyncOverview {
    source_id: &'static str,
    cache_key: &'static str,
    status: Option<String>,
    last_attempt_at: Option<String>,
    last_success_at: Option<String>,
    revision: Option<String>,
    cache_bytes: Option<i64>,
    fetched_at: Option<String>,
    content_type: Option<String>,
    last_error: Option<String>,
}

impl SourceSyncOverview {
    fn empty(source_id: &'static str, cache_key: &'static str) -> Self {
        Self {
            source_id,
            cache_key,
            status: None,
            last_attempt_at: None,
            last_success_at: None,
            revision: None,
            cache_bytes: None,
            fetched_at: None,
            content_type: None,
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

        Ok(Self::from_records(source_id, cache_key, state, cache))
    }

    fn from_records(
        source_id: &'static str,
        cache_key: &'static str,
        state: Option<SyncSourceStateRecord>,
        cache: Option<RawSourceCacheSummary>,
    ) -> Self {
        Self {
            source_id,
            cache_key,
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
            content_type: cache.as_ref().map(|value| value.content_type.clone()),
            last_error: state.and_then(|value| value.last_error),
        }
    }

    fn status_label(&self) -> &str {
        self.status.as_deref().unwrap_or("尚未同步")
    }
}

#[derive(Clone)]
struct PenguinSyncOverview {
    source: SourceSyncOverview,
    row_count: i64,
    sample_rows: Vec<ExternalDropMatrixRecord>,
}

impl PenguinSyncOverview {
    fn empty() -> Self {
        Self {
            source: SourceSyncOverview::empty(PENGUIN_MATRIX_SOURCE_ID, PENGUIN_MATRIX_CACHE_KEY),
            row_count: 0,
            sample_rows: Vec::new(),
        }
    }

    fn load(repository: &AppRepository<'_>) -> Result<Self, String> {
        Ok(Self {
            source: SourceSyncOverview::load(
                repository,
                PENGUIN_MATRIX_SOURCE_ID,
                PENGUIN_MATRIX_CACHE_KEY,
            )?,
            row_count: repository
                .count_external_drop_matrix()
                .map_err(|error| error.to_string())?,
            sample_rows: repository
                .list_external_drop_matrix(8)
                .map_err(|error| error.to_string())?,
        })
    }
}

fn run_prts_sync_task(
    database_path: &Path,
    working_directory: &Path,
) -> Result<SyncPrtsSiteInfoOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = PrtsClient::new().map_err(|error| error.to_string())?;
    sync_prts_site_info(&repository, &client, working_directory).map_err(|error| error.to_string())
}

fn run_penguin_sync_task(database_path: &Path) -> Result<SyncPenguinMatrixOutcome, String> {
    let database = AppDatabase::open(database_path).map_err(|error| error.to_string())?;
    let repository = AppRepository::new(database.connection());
    let client = PenguinClient::new().map_err(|error| error.to_string())?;
    sync_penguin_matrix(&repository, &client).map_err(|error| error.to_string())
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

fn render_source_overview(ui: &mut egui::Ui, grid_id: &str, overview: &SourceSyncOverview) {
    ui.label("这里先展示同步源的基础状态、缓存锚点和最近错误，后续再逐步压缩字段。");
    ui.separator();

    egui::Grid::new(grid_id)
        .num_columns(2)
        .spacing([24.0, 12.0])
        .show(ui, |ui| {
            overview_row(ui, "来源 ID", overview.source_id);
            overview_row(ui, "缓存键", overview.cache_key);
            overview_row(ui, "同步状态", overview.status_label());
            overview_row(ui, "最后尝试", optional_text(&overview.last_attempt_at));
            overview_row(ui, "最近成功", optional_text(&overview.last_success_at));
            overview_row(ui, "版本锚点", optional_text(&overview.revision));
            overview_row(
                ui,
                "缓存大小",
                optional_number(overview.cache_bytes, "字节"),
            );
            overview_row(ui, "缓存时间", optional_text(&overview.fetched_at));
            overview_row(ui, "内容类型", optional_text(&overview.content_type));
            overview_row(ui, "最近错误", optional_text(&overview.last_error));
        });
}

fn render_penguin_overview(ui: &mut egui::Ui, overview: &PenguinSyncOverview) {
    render_source_overview(ui, "penguin_overview", &overview.source);
    ui.separator();
    ui.label(format!("当前矩阵记录数：{}", overview.row_count));

    if overview.sample_rows.is_empty() {
        ui.label("本地数据库里还没有 Penguin 掉率矩阵记录。");
        return;
    }

    ui.label("示例矩阵记录：");
    egui::Grid::new("penguin_matrix_preview")
        .num_columns(6)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("关卡");
            ui.strong("道具");
            ui.strong("样本数");
            ui.strong("掉落总量");
            ui.strong("开始时间");
            ui.strong("结束时间");
            ui.end_row();

            for row in &overview.sample_rows {
                ui.label(row.stage_id.as_str());
                ui.label(row.item_id.as_str());
                ui.label(row.sample_count.to_string());
                ui.label(row.drop_count.to_string());
                ui.label(optional_text(&row.window_start_at));
                ui.label(optional_text(&row.window_end_at));
                ui.end_row();
            }
        });
}

fn overview_row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.label(label);
    ui.label(value.into());
    ui.end_row();
}

fn optional_text(value: &Option<String>) -> String {
    value.clone().unwrap_or_else(|| "暂无".to_string())
}

fn optional_number(value: Option<i64>, suffix: &str) -> String {
    value
        .map(|value| format!("{value} {suffix}"))
        .unwrap_or_else(|| "暂无".to_string())
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
    use super::SettingsPageState;
    use super::SyncPageState;
    use akbox_core::config::AppConfig;
    use akbox_core::config::ConfigSource;
    use akbox_core::config::LoadedConfig;
    use akbox_data::default_database_path;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
