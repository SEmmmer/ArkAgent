use std::collections::HashMap;
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
use akbox_data::ExternalEventNoticeRecord;
use akbox_data::ExternalItemDefRecord;
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
use akbox_data::SyncMode;
use akbox_data::SyncOfficialNoticeOutcome;
use akbox_data::SyncPenguinMatrixOutcome;
use akbox_data::SyncPrtsOperatorGrowthOutcome;
use akbox_data::SyncPrtsOutcome;
use akbox_data::SyncSourceStateRecord;
use akbox_data::default_database_path;
use akbox_data::sync_official_notices_with_mode;
use akbox_data::sync_penguin_matrix_with_mode;
use akbox_data::sync_prts_operator_growth_with_mode;
use akbox_data::sync_prts_with_mode;
use akbox_device::ScreenshotCaptureRequest;
use akbox_device::capture_device_screenshot_png;
use eframe::egui;
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

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
        ui.label(format!("PRTS 站点状态：{}", self.sync.prts.status_label()));
        ui.label(format!(
            "PRTS 道具状态：{}",
            self.sync.prts_items.source.status_label()
        ));
        ui.label(format!(
            "PRTS 干员状态：{}",
            self.sync.prts_operators.source.status_label()
        ));
        ui.label(format!(
            "官方公告状态：{}",
            self.sync.official.source.status_label()
        ));
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
            "当前“同步 PRTS”只覆盖站点 / 干员 / 道具 / 关卡 / 配方；其中干员 / 道具 / 关卡 / 配方，以及 Penguin 支持轻量版本锚点预检查。官方公告与 PRTS 养成需求缺少稳定增量锚点，因此即使请求增量也仍按全量执行。",
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
    force_full_sync: bool,
    selected_tab: SyncTab,
    prts: SourceSyncOverview,
    prts_items: PrtsItemIndexOverview,
    prts_operators: PrtsOperatorIndexOverview,
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
                            "请求模式：{}；实际执行：{}；结果：{}；写入 {} 条公告记录，版本锚点：{}。",
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
            Self::Official => "官方公告同步",
            Self::Penguin => "Penguin 同步",
        }
    }
}

enum SyncTaskFinished {
    Prts(Box<Result<SyncPrtsOutcome, String>>),
    PrtsGrowth(Result<SyncPrtsOperatorGrowthOutcome, String>),
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
    operator_growth: &'a PrtsOperatorGrowthOverview,
    recipe_index: &'a PrtsRecipeIndexOverview,
    stage_index: &'a PrtsStageIndexOverview,
}

fn render_prts_overview(ui: &mut egui::Ui, overview: PrtsOverviewRefs<'_>, timezone: &str) {
    ui.label(
        "PRTS 当前按六类数据展示：站点信息负责版本锚点，干员索引用于落地 `external_operator_def`（仅保留玩家 box 可拥有、可养成的正式干员；`专属干员` 与 `预备干员` 会被过滤），养成需求用于落地 `external_operator_growth`，道具索引用于落地 `external_item_def`，配方索引用于落地 `external_recipe`，关卡索引用于补齐 `external_stage_def` 的静态定义。",
    );
    ui.label(
        "当前入口已拆分：上方“同步 PRTS”只同步站点 / 干员 / 道具 / 关卡 / 配方；“同步 PRTS 养成需求”单独负责 growth。当前增量策略里，道具 / 干员 / 关卡 / 配方支持 `revision` 预检查，未变化时会跳过；养成需求来自单干员页 section，暂无稳定轻量锚点，因此仍按全量执行。",
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
    ui.label(format!("当前公告记录数：{}", overview.row_count));
    ui.label("当前仍展示全部官方公告；仅保留活动公告的过滤需求已记录，后续单独实现。");

    if overview.sample_rows.is_empty() {
        ui.label("本地数据库里还没有官方公告记录。");
        return;
    }

    ui.label("最近公告示例：");
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
    use super::build_penguin_stage_displays;
    use super::build_prts_operator_growth_display_rows;
    use akbox_core::config::AppConfig;
    use akbox_core::config::ConfigSource;
    use akbox_core::config::LoadedConfig;
    use akbox_data::ExternalOperatorGrowthRecord;
    use akbox_data::PenguinDropDisplayRecord;
    use akbox_data::default_database_path;
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
