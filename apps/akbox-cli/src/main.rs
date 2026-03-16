use std::env;
use std::path::PathBuf;
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
use akbox_data::SyncMode;
use akbox_data::default_database_path;
use akbox_data::sync_official_notices_with_mode;
use akbox_data::sync_penguin_matrix_with_mode;
use akbox_data::sync_prts_item_index_with_mode;
use akbox_data::sync_prts_operator_growth_with_mode;
use akbox_data::sync_prts_operator_index_with_mode;
use akbox_data::sync_prts_recipe_index_with_mode;
use akbox_data::sync_prts_stage_index_with_mode;
use akbox_data::sync_prts_with_mode;

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
  akbox-cli debug export-sample [path]
  akbox-cli debug --help

Debug commands:
  config         Print the resolved app configuration
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
    use super::handle_args;
    use super::parse_sync_mode_and_database_path;
    use super::render_loaded_config;
    use akbox_core::config::AppConfig;
    use akbox_data::SyncMode;

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
        assert!(result.message.contains("akbox-cli sync prts-growth"));
        assert!(result.message.contains("akbox-cli sync prts-items"));
        assert!(result.message.contains("akbox-cli sync prts-operators"));
        assert!(result.message.contains("akbox-cli sync prts-recipes"));
        assert!(result.message.contains("akbox-cli sync prts-stages"));
        assert!(result.message.contains("akbox-cli sync official"));
        assert!(result.message.contains("akbox-cli sync penguin"));
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
}
