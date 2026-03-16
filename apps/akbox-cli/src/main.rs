use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use akbox_core::config::AppConfig;
use akbox_core::config::LoadedConfig;
use akbox_core::debug_artifact::DebugArtifactExportOutcome;
use akbox_core::debug_artifact::export_sample_debug_bundle;
use akbox_core::logging::init_logging;
use akbox_data::AppDatabase;
use akbox_data::AppRepository;
use akbox_data::OfficialNoticeClient;
use akbox_data::PenguinClient;
use akbox_data::PrtsClient;
use akbox_data::SklandClient;
use akbox_data::SklandProfileRequest;
use akbox_data::SyncMode;
use akbox_data::default_database_path;
use akbox_data::discover_default_skland_auth_file;
use akbox_data::import_skland_player_info_into_operator_state;
use akbox_data::import_skland_player_info_into_status_and_building_state;
use akbox_data::inspect_skland_player_info;
use akbox_data::sync_official_notices_with_mode;
use akbox_data::sync_penguin_matrix_with_mode;
use akbox_data::sync_prts_item_index_with_mode;
use akbox_data::sync_prts_operator_building_skill_with_mode;
use akbox_data::sync_prts_operator_growth_with_mode;
use akbox_data::sync_prts_operator_index_with_mode;
use akbox_data::sync_prts_recipe_index_with_mode;
use akbox_data::sync_prts_stage_index_with_mode;
use akbox_data::sync_prts_with_mode;
use akbox_device::DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT;
use akbox_device::DeviceInputAction;
use akbox_device::DeviceInputRequest;
use akbox_device::OcrRequest;
use akbox_device::RoiPurpose;
use akbox_device::ScreenshotCaptureRequest;
use akbox_device::capture_device_screenshot;
use akbox_device::crop_all_rois_from_png;
use akbox_device::evaluate_page_confirmation_from_png;
use akbox_device::load_page_state_catalog_from_path;
use akbox_device::recognize_text_from_png;
use akbox_device::send_device_input;
use serde_json::json;

const HELP_TEXT: &str = "\
ArkAgent CLI

Usage:
  akbox-cli [command]
  akbox-cli --help

Commands:
  sync    Synchronize external data sources
  scan    Run local scan workflows
  plan    Compute planning recommendations
  debug   Development and diagnostics entry points
";

const SYNC_HELP_TEXT: &str = "\
ArkAgent CLI sync

Usage:
  akbox-cli sync prts [--full] [database_path]
  akbox-cli sync prts-building-skills [--full] [database_path]
  akbox-cli sync prts-growth [--full] [database_path]
  akbox-cli sync prts-items [--full] [database_path]
  akbox-cli sync prts-operators [--full] [database_path]
  akbox-cli sync prts-recipes [--full] [database_path]
  akbox-cli sync prts-stages [--full] [database_path]
  akbox-cli sync official [--full] [database_path]
  akbox-cli sync penguin [--full] [database_path]
  akbox-cli sync --help

Sync commands:
  prts        Default incremental; use --full to force the common PRTS sync and write site/operator/item/stage/recipe data
  prts-building-skills Default incremental request currently falls back to full because operator building skills come from per-operator sections without a stable lightweight revision anchor
  prts-growth Default incremental request currently falls back to full because operator growth has no stable lightweight revision anchor
  prts-items  Default incremental; use --full to force the PRTS item index
  prts-operators Default incremental; use --full to force the PRTS operator index
  prts-recipes Default incremental; use --full to force the PRTS workshop recipe index
  prts-stages Default incremental; use --full to force the PRTS stage index
  official    Default incremental request currently falls back to full because official notices have no stable lightweight revision anchor
  penguin     Default incremental; use --full to force the Penguin CN matrix/stages/items refresh
";

const DEBUG_HELP_TEXT: &str = "\
ArkAgent CLI debug

Usage:
  akbox-cli debug config [path]
  akbox-cli debug capture-device [--config path] [--serial serial_or_port] [output_path]
  akbox-cli debug keyevent [--config path] [--serial serial_or_port] key_code
  akbox-cli debug skland-player-info [--auth-file path] [--database path] [--import] [--import-status-building]
  akbox-cli debug vision-inspect [--templates-root path] <page_config_path> <page_id> <input_png> [output_dir]
  akbox-cli debug export-sample [path]
  akbox-cli debug --help

Debug commands:
  config         Print the resolved app configuration
  capture-device Capture a real MuMu / ADB screenshot to a PNG file
  keyevent       Send one Android keyevent through the real device chain
  skland-player-info Inspect readonly Skland player/info and optionally import into operator_state or player_status/base_building
  vision-inspect Inspect one page config against a local PNG, run template markers, crop ROIs, and export a manifest
  export-sample  Export a sample screenshot and recognition JSON
";

fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let startup_warning = bootstrap_logging(&args);
    let result = handle_args(&args);

    if let Some(warning) = startup_warning {
        eprint!("{warning}");
    }

    match result.stream {
        OutputStream::Stdout => print!("{}", result.message),
        OutputStream::Stderr => eprint!("{}", result.message),
    }

    ExitCode::from(result.exit_code)
}

fn bootstrap_logging(args: &[String]) -> Option<String> {
    if matches!(
        args.first().map(String::as_str),
        None | Some("-h") | Some("--help")
    ) {
        return None;
    }

    let (config, config_warning) = match AppConfig::load() {
        Ok(loaded) => (loaded.config, None),
        Err(error) => (
            AppConfig::default(),
            Some(format!(
                "warning: failed to load config for logging bootstrap: {error}; using default logging settings\n"
            )),
        ),
    };

    let logging_warning = match init_logging(&config) {
        Ok(state) => {
            tracing::info!(command = ?args, log_file = %state.log_file.display(), "cli command started");
            None
        }
        Err(error) => Some(format!(
            "warning: failed to initialize file logging: {error}\n"
        )),
    };

    combine_warnings(config_warning, logging_warning)
}

fn combine_warnings(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (None, None) => None,
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(second),
        (Some(first), Some(second)) => Some(format!("{first}{second}")),
    }
}

fn handle_args(args: &[String]) -> CommandResult {
    match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => CommandResult::stdout(0, help_text()),
        Some("sync") => handle_sync_args(&args[1..]),
        Some("scan" | "plan") => reserved_command(args[0].as_str()),
        Some("debug") => handle_debug_args(&args[1..]),
        Some(command) => CommandResult::stderr(
            2,
            format!("error: unsupported command `{command}`\n\n{}", help_text()),
        ),
    }
}

fn handle_sync_args(args: &[String]) -> CommandResult {
    match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => CommandResult::stdout(0, sync_help_text()),
        Some("prts") => handle_sync_prts_args(&args[1..]),
        Some("prts-building-skills") => handle_sync_prts_building_skill_args(&args[1..]),
        Some("prts-growth") => handle_sync_prts_growth_args(&args[1..]),
        Some("prts-items") => handle_sync_prts_item_args(&args[1..]),
        Some("prts-operators") => handle_sync_prts_operator_args(&args[1..]),
        Some("prts-recipes") => handle_sync_prts_recipe_args(&args[1..]),
        Some("prts-stages") => handle_sync_prts_stage_args(&args[1..]),
        Some("official") => handle_sync_official_args(&args[1..]),
        Some("penguin") => handle_sync_penguin_args(&args[1..]),
        Some(command) => CommandResult::stderr(
            2,
            format!(
                "error: unsupported sync command `{command}`\n\n{}",
                sync_help_text()
            ),
        ),
    }
}

fn parse_sync_mode_and_database_path(
    args: &[String],
    command_name: &str,
) -> Result<(SyncMode, PathBuf), CommandResult> {
    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut mode = SyncMode::Incremental;
    let mut database_path = None;

    for argument in args {
        if argument == "--full" {
            mode = SyncMode::Full;
            continue;
        }

        if database_path.is_none() {
            database_path = Some(PathBuf::from(argument));
            continue;
        }

        return Err(CommandResult::stderr(
            2,
            format!(
                "error: `akbox-cli sync {command_name}` accepts at most one database path argument plus optional `--full`\n\n{}",
                sync_help_text()
            ),
        ));
    }

    Ok((
        mode,
        database_path.unwrap_or_else(|| default_database_path(&working_directory)),
    ))
}

fn render_sync_mode_line(label: &str, mode: SyncMode) -> String {
    format!("{label}: {}\n", mode.label_zh())
}

fn render_sync_status_line(label: &str, status: akbox_data::SyncRunStatus) -> String {
    format!("{label}: {}\n", status.label_zh())
}

fn handle_sync_prts_args(args: &[String]) -> CommandResult {
    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "prts") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_with_mode(&repository, &client, &working_directory, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS sync succeeded\nDatabase: {}\n{}Site revision: {}\nSite cached bytes: {}\nOperator revision: {}\nOperator count: {}\n{}{}Item revision: {}\nItem count: {}\n{}{}Stage revision: {}\nStage count: {}\n{}{}Recipe revision: {}\nRecipe count: {}\n{}{}",
                database.path().display(),
                render_sync_mode_line("Requested mode", mode),
                outcome.site_info.revision,
                outcome.site_info.cache_size_bytes,
                outcome.operator_index.revision,
                outcome.operator_index.row_count,
                render_sync_mode_line("Operator mode", outcome.operator_index.effective_mode),
                render_sync_status_line("Operator status", outcome.operator_index.run_status),
                outcome.item_index.revision,
                outcome.item_index.row_count,
                render_sync_mode_line("Item mode", outcome.item_index.effective_mode),
                render_sync_status_line("Item status", outcome.item_index.run_status),
                outcome.stage_index.revision,
                outcome.stage_index.row_count,
                render_sync_mode_line("Stage mode", outcome.stage_index.effective_mode),
                render_sync_status_line("Stage status", outcome.stage_index.run_status),
                outcome.recipe_index.revision,
                outcome.recipe_index.row_count,
                render_sync_mode_line("Recipe mode", outcome.recipe_index.effective_mode),
                render_sync_status_line("Recipe status", outcome.recipe_index.run_status),
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_prts_building_skill_args(args: &[String]) -> CommandResult {
    let (mode, database_path) =
        match parse_sync_mode_and_database_path(args, "prts-building-skills") {
            Ok(value) => value,
            Err(result) => return result,
        };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_operator_building_skill_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS operator building skill sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nBuilding skill row count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_prts_growth_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "prts-growth") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_operator_growth_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS operator growth sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nGrowth row count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_prts_operator_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "prts-operators") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_operator_index_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS operator index sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nOperator count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_prts_item_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "prts-items") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_item_index_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS item index sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nItem count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_prts_stage_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "prts-stages") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_stage_index_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS stage index sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nStage count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_prts_recipe_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "prts-recipes") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PrtsClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_prts_recipe_index_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "PRTS recipe index sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nRecipe count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_official_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "official") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match OfficialNoticeClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_official_notices_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "Official notice sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nNotice count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_sync_penguin_args(args: &[String]) -> CommandResult {
    let (mode, database_path) = match parse_sync_mode_and_database_path(args, "penguin") {
        Ok(value) => value,
        Err(result) => return result,
    };

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match PenguinClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    match sync_penguin_matrix_with_mode(&repository, &client, mode) {
        Ok(outcome) => CommandResult::stdout(
            0,
            format!(
                "Penguin sync succeeded\nDatabase: {}\nSource id: {}\nCache key: {}\nRequested mode: {}\nEffective mode: {}\nRun status: {}\nRevision: {}\nCached bytes: {}\nRow count: {}\n",
                database.path().display(),
                outcome.source_id,
                outcome.cache_key,
                outcome.requested_mode.label_zh(),
                outcome.effective_mode.label_zh(),
                outcome.run_status.label_zh(),
                outcome.revision,
                outcome.cache_size_bytes,
                outcome.row_count
            ),
        ),
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_debug_args(args: &[String]) -> CommandResult {
    match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => CommandResult::stdout(0, debug_help_text()),
        Some("config") => handle_debug_config_args(&args[1..]),
        Some("capture-device") => handle_debug_capture_device_args(&args[1..]),
        Some("keyevent") => handle_debug_keyevent_args(&args[1..]),
        Some("skland-player-info") => handle_debug_skland_player_info_args(&args[1..]),
        Some("vision-inspect") => handle_debug_vision_inspect_args(&args[1..]),
        Some("export-sample") => handle_debug_export_sample_args(&args[1..]),
        Some(command) => CommandResult::stderr(
            2,
            format!(
                "error: unsupported debug command `{command}`\n\n{}",
                debug_help_text()
            ),
        ),
    }
}

fn handle_debug_config_args(args: &[String]) -> CommandResult {
    let loaded = match load_debug_config(args, "config") {
        Ok(loaded) => loaded,
        Err(result) => return result,
    };

    CommandResult::stdout(0, render_loaded_config(&loaded))
}

fn handle_debug_export_sample_args(args: &[String]) -> CommandResult {
    let loaded = match load_debug_config(args, "export-sample") {
        Ok(loaded) => loaded,
        Err(result) => return result,
    };
    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match export_sample_debug_bundle(&loaded.config, &working_directory, "cli-debug") {
        Ok(bundle) => {
            let screenshot = render_artifact_outcome("Screenshot", &bundle.screenshot);
            let recognition = render_artifact_outcome("Recognition", &bundle.recognition);
            CommandResult::stdout(
                0,
                format!(
                    "Debug export directory: {}\n{screenshot}\n{recognition}\n",
                    bundle.directory.display()
                ),
            )
        }
        Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
    }
}

fn handle_debug_capture_device_args(args: &[String]) -> CommandResult {
    let mut config_path = None;
    let mut preferred_serial = None;
    let mut output_path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--config` requires a path value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                config_path = Some(PathBuf::from(value));
            }
            "--serial" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--serial` requires a serial or port value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                preferred_serial = Some(value.clone());
            }
            value => {
                if output_path.is_some() {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `akbox-cli debug capture-device` accepts at most one output path\n\n{}",
                            debug_help_text()
                        ),
                    );
                }
                output_path = Some(PathBuf::from(value));
            }
        }

        index += 1;
    }

    let loaded = match config_path {
        Some(path) => match AppConfig::load_or_default_from(path) {
            Ok(loaded) => loaded,
            Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
        },
        None => AppConfig::load().unwrap_or_else(|_| LoadedConfig {
            source: akbox_core::config::ConfigSource::Defaults {
                expected_path: AppConfig::default_config_path()
                    .unwrap_or_else(|_| PathBuf::from("ArkAgent.toml")),
            },
            config: AppConfig::default(),
        }),
    };

    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let output_path = output_path
        .unwrap_or_else(|| working_directory.join("debug-artifacts/cli-device-capture.png"));
    if let Some(parent) = output_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return CommandResult::stderr(
            1,
            format!(
                "error: failed to create output directory {}: {error}\n",
                parent.display()
            ),
        );
    }

    let request = ScreenshotCaptureRequest {
        adb_executable: loaded.config.adb.executable.clone(),
        preferred_serial,
        discovery_instance_count: DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT,
    };
    let captured = match capture_device_screenshot(&request) {
        Ok(captured) => captured,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    if let Err(error) = fs::write(&output_path, &captured.png_bytes) {
        return CommandResult::stderr(
            1,
            format!(
                "error: failed to write screenshot to {}: {error}\n",
                output_path.display()
            ),
        );
    }

    CommandResult::stdout(
        0,
        format!(
            "Device screenshot captured\nOutput: {}\nADB: {}\nSerial: {}\nSelection source: {}\nPNG bytes: {}\n",
            output_path.display(),
            captured.connection.adb_executable.display(),
            captured.connection.serial,
            captured.connection.selection_source.label_zh(),
            captured.png_bytes.len()
        ),
    )
}

fn handle_debug_keyevent_args(args: &[String]) -> CommandResult {
    let mut config_path = None;
    let mut preferred_serial = None;
    let mut key_code = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--config` requires a path value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                config_path = Some(PathBuf::from(value));
            }
            "--serial" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--serial` requires a serial or port value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                preferred_serial = Some(value.clone());
            }
            value => {
                if key_code.is_some() {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `akbox-cli debug keyevent` accepts exactly one key code\n\n{}",
                            debug_help_text()
                        ),
                    );
                }
                key_code = Some(value.to_string());
            }
        }

        index += 1;
    }

    let Some(key_code_text) = key_code else {
        return CommandResult::stderr(
            2,
            format!(
                "error: `akbox-cli debug keyevent` requires one key code\n\n{}",
                debug_help_text()
            ),
        );
    };
    let key_code = match key_code_text.parse::<u16>() {
        Ok(value) => value,
        Err(_) => {
            return CommandResult::stderr(2, "error: key code must be an integer in 0..65535\n");
        }
    };

    let loaded = match config_path {
        Some(path) => match AppConfig::load_or_default_from(path) {
            Ok(loaded) => loaded,
            Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
        },
        None => AppConfig::load().unwrap_or_else(|_| LoadedConfig {
            source: akbox_core::config::ConfigSource::Defaults {
                expected_path: AppConfig::default_config_path()
                    .unwrap_or_else(|_| PathBuf::from("ArkAgent.toml")),
            },
            config: AppConfig::default(),
        }),
    };

    let request = DeviceInputRequest {
        adb_executable: loaded.config.adb.executable.clone(),
        preferred_serial,
        discovery_instance_count: DEFAULT_MUMU_DISCOVERY_INSTANCE_COUNT,
        action: DeviceInputAction::KeyEvent { key_code },
    };
    let result = match send_device_input(&request) {
        Ok(result) => result,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    CommandResult::stdout(
        0,
        format!(
            "Device keyevent sent\nADB: {}\nSerial: {}\nSelection source: {}\nKey code: {}\n",
            result.connection.adb_executable.display(),
            result.connection.serial,
            result.connection.selection_source.label_zh(),
            key_code
        ),
    )
}

fn handle_debug_skland_player_info_args(args: &[String]) -> CommandResult {
    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut auth_file_path = None;
    let mut database_path = None;
    let mut should_import = false;
    let mut should_import_status_building = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--auth-file" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--auth-file` requires a path value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                auth_file_path = Some(PathBuf::from(value));
            }
            "--database" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--database` requires a path value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                database_path = Some(PathBuf::from(value));
            }
            "--import" => should_import = true,
            "--import-status-building" => should_import_status_building = true,
            other => {
                return CommandResult::stderr(
                    2,
                    format!(
                        "error: unsupported argument `{other}` for `akbox-cli debug skland-player-info`\n\n{}",
                        debug_help_text()
                    ),
                );
            }
        }

        index += 1;
    }

    let auth_file_path = match auth_file_path {
        Some(path) => path,
        None => match discover_default_skland_auth_file(&working_directory) {
            Some(path) => path,
            None => {
                return CommandResult::stderr(
                    1,
                    format!(
                        "error: no default skland auth file found from {}; use `--auth-file`\n",
                        working_directory.display()
                    ),
                );
            }
        },
    };
    let database_path = database_path.unwrap_or_else(|| default_database_path(&working_directory));

    if should_import && should_import_status_building {
        return CommandResult::stderr(
            2,
            format!(
                "error: `--import` and `--import-status-building` cannot be used together\n\n{}",
                debug_help_text()
            ),
        );
    }

    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let repository = AppRepository::new(database.connection());
    let client = match SklandClient::new() {
        Ok(client) => client,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let request = SklandProfileRequest {
        auth_file_path: auth_file_path.clone(),
    };

    if should_import {
        match import_skland_player_info_into_operator_state(&repository, &client, &request) {
            Ok(outcome) => CommandResult::stdout(
                0,
                format!(
                    "Skland operator import succeeded\nAuth file: {}\nDatabase: {}\nRevision: {}\nUID: {}\nBinding count: {}\nCharacter count: {}\nAssist count: {}\nEquipment info count: {}\nChar info count: {}\nStatus keys: {}\nStore TS: {}\nHas building: {}\nBuilding keys: {}\nBuilding summary: {}\nSample operator: {}\nSnapshot: {}\nImported rows: {}\nOwned rows: {}\nUnowned rows: {}\nUsed external defs: {}\n",
                    auth_file_path.display(),
                    database.path().display(),
                    outcome.inspect.revision,
                    outcome.inspect.uid,
                    outcome.inspect.binding_count,
                    outcome.inspect.char_count,
                    outcome.inspect.assist_count,
                    outcome.inspect.equipment_info_count,
                    outcome.inspect.char_info_count,
                    outcome.inspect.status_keys.join(", "),
                    render_optional_i64(outcome.inspect.status_store_ts),
                    if outcome.inspect.has_building {
                        "yes"
                    } else {
                        "no"
                    },
                    outcome.inspect.building_keys.join(", "),
                    render_skland_building_summary(&outcome.inspect),
                    render_skland_sample_operator(outcome.inspect.sample_operator.as_ref()),
                    outcome.snapshot_id,
                    outcome.imported_row_count,
                    outcome.owned_row_count,
                    outcome.unowned_row_count,
                    if outcome.used_external_operator_defs {
                        "yes"
                    } else {
                        "no"
                    },
                ),
            ),
            Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
        }
    } else if should_import_status_building {
        match import_skland_player_info_into_status_and_building_state(
            &repository,
            &client,
            &request,
        ) {
            Ok(outcome) => CommandResult::stdout(
                0,
                format!(
                    "Skland status/building import succeeded\nAuth file: {}\nDatabase: {}\nRevision: {}\nUID: {}\nAccount name: {}\nBinding count: {}\nStatus keys: {}\nStore TS: {}\nHas building: {}\nBuilding keys: {}\nBuilding summary: {}\nPlayer status snapshot: {}\nBase building snapshot: {}\n",
                    auth_file_path.display(),
                    database.path().display(),
                    outcome.inspect.revision,
                    outcome.inspect.uid,
                    outcome
                        .inspect
                        .account_name
                        .as_deref()
                        .unwrap_or("<unknown>"),
                    outcome.inspect.binding_count,
                    outcome.inspect.status_keys.join(", "),
                    render_optional_i64(outcome.inspect.status_store_ts),
                    if outcome.inspect.has_building {
                        "yes"
                    } else {
                        "no"
                    },
                    outcome.inspect.building_keys.join(", "),
                    render_skland_building_summary(&outcome.inspect),
                    outcome.player_status_snapshot_id,
                    outcome.base_building_snapshot_id,
                ),
            ),
            Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
        }
    } else {
        match inspect_skland_player_info(&repository, &client, &request) {
            Ok(outcome) => CommandResult::stdout(
                0,
                format!(
                    "Skland player/info inspect succeeded\nAuth file: {}\nDatabase: {}\nRevision: {}\nUID: {}\nAccount name: {}\nBinding count: {}\nCharacter count: {}\nAssist count: {}\nEquipment info count: {}\nChar info count: {}\nStatus keys: {}\nStore TS: {}\nHas building: {}\nBuilding keys: {}\nBuilding summary: {}\nSample operator: {}\nCached bytes: {}\n",
                    auth_file_path.display(),
                    database.path().display(),
                    outcome.revision,
                    outcome.uid,
                    outcome.account_name.as_deref().unwrap_or("<unknown>"),
                    outcome.binding_count,
                    outcome.char_count,
                    outcome.assist_count,
                    outcome.equipment_info_count,
                    outcome.char_info_count,
                    outcome.status_keys.join(", "),
                    render_optional_i64(outcome.status_store_ts),
                    if outcome.has_building { "yes" } else { "no" },
                    outcome.building_keys.join(", "),
                    render_skland_building_summary(&outcome),
                    render_skland_sample_operator(outcome.sample_operator.as_ref()),
                    outcome.cache_size_bytes,
                ),
            ),
            Err(error) => CommandResult::stderr(1, format!("error: {error}\n")),
        }
    }
}

fn handle_debug_vision_inspect_args(args: &[String]) -> CommandResult {
    let mut templates_root = None;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--templates-root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return CommandResult::stderr(
                        2,
                        format!(
                            "error: `--templates-root` requires a path value\n\n{}",
                            debug_help_text()
                        ),
                    );
                };
                templates_root = Some(PathBuf::from(value));
            }
            value => positional.push(value.to_string()),
        }

        index += 1;
    }

    let [page_config_path, page_id, input_png, ..] = positional.as_slice() else {
        return CommandResult::stderr(
            2,
            format!(
                "error: `akbox-cli debug vision-inspect` requires <page_config_path> <page_id> <input_png> and optional [output_dir]\n\n{}",
                debug_help_text()
            ),
        );
    };
    if positional.len() > 4 {
        return CommandResult::stderr(
            2,
            format!(
                "error: `akbox-cli debug vision-inspect` accepts at most one optional output_dir\n\n{}",
                debug_help_text()
            ),
        );
    }

    let page_config_path = PathBuf::from(page_config_path);
    let input_png = PathBuf::from(input_png);
    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let output_dir = positional.get(3).map(PathBuf::from).unwrap_or_else(|| {
        working_directory
            .join("debug-artifacts")
            .join("vision-inspect")
            .join(page_id)
    });
    if let Err(error) = fs::create_dir_all(&output_dir) {
        return CommandResult::stderr(
            1,
            format!(
                "error: failed to create vision output directory {}: {error}\n",
                output_dir.display()
            ),
        );
    }

    let catalog = match load_page_state_catalog_from_path(&page_config_path) {
        Ok(catalog) => catalog,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };
    let Some(page) = catalog.find_page(page_id).cloned() else {
        return CommandResult::stderr(
            1,
            format!(
                "error: page `{page_id}` not found in config {}\n",
                page_config_path.display()
            ),
        );
    };

    let templates_root =
        templates_root.unwrap_or_else(|| default_templates_root_for_config(&page_config_path));
    let screenshot = match fs::read(&input_png) {
        Ok(bytes) => bytes,
        Err(error) => {
            return CommandResult::stderr(
                1,
                format!(
                    "error: failed to read input PNG {}: {error}\n",
                    input_png.display()
                ),
            );
        }
    };

    let confirmation =
        match evaluate_page_confirmation_from_png(&page, &screenshot, &templates_root) {
            Ok(result) => result,
            Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
        };
    let crops = match crop_all_rois_from_png(&page, &screenshot) {
        Ok(crops) => crops,
        Err(error) => return CommandResult::stderr(1, format!("error: {error}\n")),
    };

    let mut roi_manifest = Vec::new();
    for crop in &crops {
        let file_name = format!("{}.png", sanitize_debug_file_name(&crop.roi.roi_id));
        let output_path = output_dir.join(&file_name);
        if let Err(error) = fs::write(&output_path, &crop.png_bytes) {
            return CommandResult::stderr(
                1,
                format!(
                    "error: failed to write ROI PNG {}: {error}\n",
                    output_path.display()
                ),
            );
        }

        let ocr_result = match crop.roi.purpose {
            RoiPurpose::NumericOcr | RoiPurpose::ShortTextOcr => {
                let request = OcrRequest {
                    numeric_only: matches!(crop.roi.purpose, RoiPurpose::NumericOcr),
                    ..OcrRequest::default()
                };
                match recognize_text_from_png(&crop.png_bytes, &request) {
                    Ok(result) => json!({
                        "status": "ok",
                        "backend": result.backend,
                        "text": result.text,
                        "lines": result.lines,
                    }),
                    Err(error) => json!({
                        "status": "error",
                        "message": error.to_string(),
                    }),
                }
            }
            _ => json!({
                "status": "skipped",
                "message": "当前 ROI purpose 不走 OCR"
            }),
        };

        roi_manifest.push(json!({
            "roi_id": crop.roi.roi_id,
            "display_name": crop.roi.display_name,
            "purpose": crop.roi.purpose,
            "output_png": output_path.display().to_string(),
            "artifact_payload": crop.artifact_payload(),
            "ocr": ocr_result,
        }));
    }

    let manifest = json!({
        "page_config_path": page_config_path.display().to_string(),
        "page_id": page.page_id,
        "templates_root": templates_root.display().to_string(),
        "input_png": input_png.display().to_string(),
        "page_confirmation": confirmation,
        "roi_outputs": roi_manifest,
    });
    let manifest_path = output_dir.join("manifest.json");
    let manifest_json = match serde_json::to_string_pretty(&manifest) {
        Ok(json) => json,
        Err(error) => {
            return CommandResult::stderr(
                1,
                format!("error: failed to serialize vision manifest: {error}\n"),
            );
        }
    };
    if let Err(error) = fs::write(&manifest_path, manifest_json) {
        return CommandResult::stderr(
            1,
            format!(
                "error: failed to write vision manifest {}: {error}\n",
                manifest_path.display()
            ),
        );
    }

    CommandResult::stdout(
        0,
        format!(
            "Vision inspect succeeded\nPage config: {}\nPage id: {}\nInput PNG: {}\nTemplates root: {}\nPage matched: {}\nMarker match count: {}/{}\nROI output count: {}\nManifest: {}\nOutput directory: {}\n",
            page_config_path.display(),
            page_id,
            input_png.display(),
            templates_root.display(),
            if confirmation.matched { "yes" } else { "no" },
            confirmation.matched_markers,
            confirmation.total_markers,
            crops.len(),
            manifest_path.display(),
            output_dir.display()
        ),
    )
}

fn load_debug_config(args: &[String], command_name: &str) -> Result<LoadedConfig, CommandResult> {
    let loaded = match args {
        [] => AppConfig::load(),
        [path] => AppConfig::load_from_path(PathBuf::from(path)),
        _ => {
            return Err(CommandResult::stderr(
                2,
                format!(
                    "error: `akbox-cli debug {command_name}` accepts at most one path argument\n\n{}",
                    debug_help_text()
                ),
            ));
        }
    };

    loaded.map_err(|error| CommandResult::stderr(1, format!("error: {error}\n")))
}

fn render_loaded_config(loaded: &LoadedConfig) -> String {
    let working_directory = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let adb_executable = loaded
        .config
        .adb
        .executable
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<auto-discover>".to_string());
    let log_file = loaded.config.logging.resolved_file_path(&working_directory);
    let debug_export_directory = loaded
        .config
        .debug
        .resolved_export_directory(&working_directory);

    format!(
        "Config source: {}\nConfig path: {}\nADB executable: {adb_executable}\nGame timezone: {}\nLog file: {}\nDebug export enabled: {}\nDebug export directory: {}\n",
        loaded.source.describe(),
        loaded.save_path().display(),
        loaded.config.game.timezone,
        log_file.display(),
        loaded.config.debug.export_artifacts,
        debug_export_directory.display()
    )
}

fn render_artifact_outcome(label: &str, outcome: &DebugArtifactExportOutcome) -> String {
    match outcome {
        DebugArtifactExportOutcome::Disabled { directory } => format!(
            "{label}: skipped because debug export is disabled in config (target directory: {})",
            directory.display()
        ),
        DebugArtifactExportOutcome::Exported(file) => format!(
            "{label}: exported to {} ({} bytes)",
            file.path.display(),
            file.bytes_written
        ),
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

fn render_skland_sample_operator(sample: Option<&akbox_data::SklandOperatorSample>) -> String {
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

fn render_optional_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

fn render_skland_building_summary(inspect: &akbox_data::SklandPlayerInfoInspectOutcome) -> String {
    format!(
        "control={} meeting={} training={} hire={} dorms={} manuf={} trade={} power={} tired={}",
        yes_no(inspect.has_control),
        yes_no(inspect.has_meeting),
        yes_no(inspect.has_training),
        yes_no(inspect.has_hire),
        inspect.dormitory_count,
        inspect.manufacture_count,
        inspect.trading_count,
        inspect.power_count,
        inspect.tired_char_count
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn reserved_command(command: &str) -> CommandResult {
    CommandResult::stdout(
        0,
        format!(
            "The `{command}` command is reserved for a future milestone.\n\
             Use `akbox-cli --help` to inspect the planned interface.\n"
        ),
    )
}

fn help_text() -> &'static str {
    HELP_TEXT
}

fn sync_help_text() -> &'static str {
    SYNC_HELP_TEXT
}

fn debug_help_text() -> &'static str {
    DEBUG_HELP_TEXT
}

struct CommandResult {
    exit_code: u8,
    message: String,
    stream: OutputStream,
}

impl CommandResult {
    fn stdout(exit_code: u8, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
            stream: OutputStream::Stdout,
        }
    }

    fn stderr(exit_code: u8, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
            stream: OutputStream::Stderr,
        }
    }
}

enum OutputStream {
    Stdout,
    Stderr,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::OutputStream;
    use super::default_templates_root_for_config;
    use super::handle_args;
    use super::parse_sync_mode_and_database_path;
    use super::render_loaded_config;
    use akbox_core::config::AppConfig;
    use akbox_data::SyncMode;
    use image::DynamicImage;
    use image::ImageBuffer;
    use image::ImageFormat;
    use image::Rgba;
    use std::io::Cursor;

    #[test]
    fn prints_help_when_no_args_are_provided() {
        let result = handle_args(&[]);
        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("ArkAgent CLI"));
    }

    #[test]
    fn prints_help_for_help_flag() {
        let result = handle_args(&[String::from("--help")]);
        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("Commands"));
    }

    #[test]
    fn keeps_planned_subcommands_stable() {
        let result = handle_args(&[String::from("plan")]);
        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("reserved for a future milestone"));
    }

    #[test]
    fn sync_help_is_available() {
        let result = handle_args(&[String::from("sync"), String::from("--help")]);
        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("akbox-cli sync prts"));
        assert!(
            result
                .message
                .contains("akbox-cli sync prts-building-skills")
        );
        assert!(result.message.contains("akbox-cli sync prts-growth"));
        assert!(result.message.contains("akbox-cli sync prts-items"));
        assert!(result.message.contains("akbox-cli sync prts-operators"));
        assert!(result.message.contains("akbox-cli sync prts-recipes"));
        assert!(result.message.contains("akbox-cli sync prts-stages"));
        assert!(result.message.contains("akbox-cli sync official"));
        assert!(result.message.contains("akbox-cli sync penguin"));
    }

    #[test]
    fn debug_help_mentions_skland_player_info() {
        let result = handle_args(&[String::from("debug"), String::from("--help")]);
        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(
            result
                .message
                .contains("akbox-cli debug skland-player-info")
        );
    }

    #[test]
    fn sync_prts_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_mode_parser_accepts_full_flag_and_path() {
        let parsed = parse_sync_mode_and_database_path(
            &[String::from("--full"), String::from("custom.db")],
            "prts",
        );
        let (mode, path) = match parsed {
            Ok(value) => value,
            Err(result) => panic!("expected parser success, got: {}", result.message),
        };

        assert_eq!(mode, SyncMode::Full);
        assert_eq!(path, PathBuf::from("custom.db"));
    }

    #[test]
    fn sync_official_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("official"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_prts_items_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts-items"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_prts_growth_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts-growth"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_prts_building_skills_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts-building-skills"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_prts_operators_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts-operators"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_prts_recipes_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts-recipes"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_prts_stages_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("prts-stages"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn sync_penguin_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("sync"),
            String::from("penguin"),
            String::from("one.db"),
            String::from("two.db"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("accepts at most one database path argument")
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        let result = handle_args(&[String::from("unknown")]);
        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(result.message.contains("unsupported command"));
    }

    #[test]
    fn debug_config_reads_explicit_file() {
        let dir = unique_test_path("cli-config");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ArkAgent.toml");
        fs::write(
            &path,
            "[adb]\nexecutable = 'D:/MuMu/adb.exe'\n\n[game]\ntimezone = 'UTC'\n",
        )
        .unwrap();

        let result = handle_args(&[
            String::from("debug"),
            String::from("config"),
            path.display().to_string(),
        ]);

        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("Config source: file"));
        assert!(result.message.contains("D:/MuMu/adb.exe"));
        assert!(result.message.contains("Game timezone: UTC"));
        assert!(result.message.contains("Log file:"));

        fs::remove_file(path).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn debug_config_rejects_too_many_paths() {
        let result = handle_args(&[
            String::from("debug"),
            String::from("config"),
            String::from("one.toml"),
            String::from("two.toml"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(result.message.contains("accepts at most one path argument"));
    }

    #[test]
    fn debug_help_lists_capture_device() {
        let result = handle_args(&[String::from("debug"), String::from("--help")]);

        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("capture-device"));
        assert!(result.message.contains("keyevent"));
        assert!(result.message.contains("vision-inspect"));
    }

    #[test]
    fn debug_capture_device_rejects_multiple_output_paths() {
        let result = handle_args(&[
            String::from("debug"),
            String::from("capture-device"),
            String::from("one.png"),
            String::from("two.png"),
        ]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(result.message.contains("accepts at most one output path"));
    }

    #[test]
    fn debug_keyevent_requires_one_key_code() {
        let result = handle_args(&[String::from("debug"), String::from("keyevent")]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(result.message.contains("requires one key code"));
    }

    #[test]
    fn debug_vision_inspect_rejects_missing_required_arguments() {
        let result = handle_args(&[String::from("debug"), String::from("vision-inspect")]);

        assert_eq!(result.exit_code, 2);
        assert!(matches!(result.stream, OutputStream::Stderr));
        assert!(
            result
                .message
                .contains("requires <page_config_path> <page_id> <input_png>")
        );
    }

    #[test]
    fn debug_vision_inspect_runs_and_writes_manifest() {
        let dir = unique_test_path("cli-vision-inspect");
        let templates_root = dir.join("assets").join("templates");
        let pages_dir = templates_root.join("pages");
        let markers_dir = templates_root.join("markers").join("inventory_main");
        fs::create_dir_all(&pages_dir).unwrap();
        fs::create_dir_all(&markers_dir).unwrap();

        let page_config_path = pages_dir.join("inventory_main.json");
        fs::write(
            &page_config_path,
            r#"{
  "pages": [
    {
      "page_id": "inventory_main",
      "display_name": "仓库主页",
      "reference_resolution": { "width": 4, "height": 4 },
      "confirmation_markers": [
        {
          "marker_id": "title_marker",
          "rect": { "x": 0, "y": 0, "width": 2, "height": 1 },
          "strategy": "template_fingerprint",
          "match_method": "normalized_grayscale_mae",
          "template_path": "markers/inventory_main/title_marker.png",
          "pass_threshold": 0.98
        }
      ],
      "rois": [
        {
          "roi_id": "item_count",
          "display_name": "物品数量",
          "rect": { "x": 2, "y": 0, "width": 2, "height": 2 },
          "purpose": "numeric_ocr",
          "preprocess_steps": [
            { "kind": "grayscale" },
            { "kind": "upscale2x" }
          ],
          "confidence_threshold": 0.98,
          "low_confidence_policy": "queue_review"
        }
      ]
    }
  ]
}"#,
        )
        .unwrap();
        fs::write(markers_dir.join("title_marker.png"), sample_red_png(2, 1)).unwrap();

        let screenshot_path = dir.join("inventory_main.png");
        fs::write(&screenshot_path, sample_inventory_png()).unwrap();
        let output_dir = dir.join("vision-output");

        let result = handle_args(&[
            String::from("debug"),
            String::from("vision-inspect"),
            page_config_path.display().to_string(),
            String::from("inventory_main"),
            screenshot_path.display().to_string(),
            output_dir.display().to_string(),
        ]);

        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("Vision inspect succeeded"));
        assert!(result.message.contains("Page matched: yes"));
        assert!(output_dir.join("item_count.png").is_file());
        let manifest = fs::read_to_string(output_dir.join("manifest.json")).unwrap();
        assert!(manifest.contains("\"matched\": true"));
        assert!(manifest.contains("\"item_count\""));
        assert!(
            manifest.contains("\"status\": \"ok\"") || manifest.contains("\"status\": \"error\"")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_templates_root_uses_parent_of_pages_directory() {
        let path = PathBuf::from("assets/templates/pages/inventory_main.json");

        let root = default_templates_root_for_config(&path);

        assert_eq!(root, PathBuf::from("assets/templates"));
    }

    #[test]
    fn debug_export_sample_writes_files_when_enabled() {
        let dir = unique_test_path("cli-export");
        let export_directory = dir.join("captures");
        fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("ArkAgent.toml");
        let export_directory_toml = export_directory.display().to_string().replace('\\', "/");
        fs::write(
            &config_path,
            format!(
                "[debug]\nexport_artifacts = true\nexport_directory = '{export_directory_toml}'\n"
            ),
        )
        .unwrap();

        let result = handle_args(&[
            String::from("debug"),
            String::from("export-sample"),
            config_path.display().to_string(),
        ]);

        assert_eq!(result.exit_code, 0);
        assert!(matches!(result.stream, OutputStream::Stdout));
        assert!(result.message.contains("Debug export directory:"));

        let exported_files = fs::read_dir(&export_directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();

        assert_eq!(exported_files.len(), 2);
        assert!(
            exported_files
                .iter()
                .any(|path| path.extension().is_some_and(|ext| ext == "png"))
        );
        assert!(
            exported_files
                .iter()
                .any(|path| path.extension().is_some_and(|ext| ext == "json"))
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rendered_config_includes_debug_and_logging_paths() {
        let dir = unique_test_path("cli-rendered-config");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ArkAgent.toml");
        let saved_path = AppConfig::default().save_to_path(&path).unwrap();
        let loaded = AppConfig::load_from_path(&saved_path).unwrap();

        let rendered = render_loaded_config(&loaded);

        assert!(rendered.contains("Log file:"));
        assert!(rendered.contains("Debug export directory:"));

        fs::remove_file(path).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-cli-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn sample_inventory_png() -> Vec<u8> {
        let image = ImageBuffer::from_fn(4, 4, |x, y| {
            if x < 2 && y < 2 {
                Rgba([255, 0, 0, 255])
            } else if x >= 2 && y < 2 {
                Rgba([255, 255, 255, 255])
            } else if x < 2 && y >= 2 {
                Rgba([0, 0, 255, 255])
            } else {
                Rgba([0, 255, 0, 255])
            }
        });
        encode_png(image)
    }

    fn sample_red_png(width: u32, height: u32) -> Vec<u8> {
        encode_png(ImageBuffer::from_pixel(
            width,
            height,
            Rgba([255, 0, 0, 255]),
        ))
    }

    fn encode_png(image: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }
}
