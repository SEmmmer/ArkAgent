use std::env;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use thiserror::Error;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::AppConfig;

static LOGGING_STATE: OnceLock<LoggingState> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingState {
    pub log_file: PathBuf,
}

pub fn init_logging(config: &AppConfig) -> Result<&'static LoggingState, LoggingError> {
    if let Some(state) = LOGGING_STATE.get() {
        return Ok(state);
    }

    let working_dir =
        env::current_dir().map_err(|source| LoggingError::CurrentDirectory { source })?;
    let log_file = config.logging.resolved_file_path(&working_dir);
    let log_directory = log_file
        .parent()
        .ok_or_else(|| LoggingError::InvalidLogPath {
            path: log_file.clone(),
        })?
        .to_path_buf();

    fs::create_dir_all(&log_directory).map_err(|source| LoggingError::CreateDirectory {
        path: log_directory,
        source,
    })?;

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .map_err(|source| LoggingError::OpenFile {
            path: log_file.clone(),
            source,
        })?;

    let writer = SharedFileWriter::new(file);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_filter(LevelFilter::INFO),
        )
        .try_init()
        .map_err(|source| LoggingError::InstallSubscriber { source })?;

    let state = LoggingState {
        log_file: log_file.clone(),
    };
    let _ = LOGGING_STATE.set(state);

    let state = LOGGING_STATE
        .get()
        .expect("logging state must be initialized");
    tracing::info!(log_file = %state.log_file.display(), "logging initialized");
    Ok(state)
}

pub fn active_logging() -> Option<&'static LoggingState> {
    LOGGING_STATE.get()
}

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("failed to determine current working directory for logging: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("invalid log file path `{path}`")]
    InvalidLogPath { path: PathBuf },
    #[error("failed to create log directory `{path}`: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("failed to open log file `{path}`: {source}")]
    OpenFile { path: PathBuf, source: io::Error },
    #[error("failed to install tracing subscriber: {source}")]
    InstallSubscriber {
        source: tracing_subscriber::util::TryInitError,
    },
}

#[derive(Clone)]
struct SharedFileWriter {
    file: Arc<Mutex<File>>,
}

impl SharedFileWriter {
    fn new(file: File) -> Self {
        Self {
            file: Arc::new(Mutex::new(file)),
        }
    }
}

impl<'a> MakeWriter<'a> for SharedFileWriter {
    type Writer = SharedFileGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        SharedFileGuard {
            guard: self.file.lock().expect("log file mutex poisoned"),
        }
    }
}

struct SharedFileGuard<'a> {
    guard: MutexGuard<'a, File>,
}

impl io::Write for SharedFileGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.guard.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.guard.flush()
    }
}
