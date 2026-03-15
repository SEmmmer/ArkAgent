use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_CONFIG_FILE_NAME: &str = "ArkAgent.toml";
pub const DEFAULT_GAME_TIMEZONE: &str = "Asia/Shanghai";
pub const DEFAULT_LOG_DIRECTORY_NAME: &str = "logs";
pub const DEFAULT_LOG_FILE_NAME: &str = "arkagent.log";
pub const DEFAULT_DEBUG_EXPORT_DIRECTORY_NAME: &str = "debug-artifacts";

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub adb: AdbConfig,
    pub game: GameConfig,
    pub logging: LoggingConfig,
    pub debug: DebugConfig,
}

impl AppConfig {
    pub fn load() -> Result<LoadedConfig, ConfigError> {
        Self::load_or_default_from(Self::default_config_path()?)
    }

    pub fn load_from_path(path: impl Into<PathBuf>) -> Result<LoadedConfig, ConfigError> {
        let path = path.into();
        let contents = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config = Self::from_toml_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;

        Ok(LoadedConfig {
            source: ConfigSource::File(path),
            config,
        })
    }

    pub fn load_or_default_from(path: impl Into<PathBuf>) -> Result<LoadedConfig, ConfigError> {
        let path = path.into();

        if path.is_file() {
            return Self::load_from_path(path);
        }

        Ok(LoadedConfig {
            source: ConfigSource::Defaults {
                expected_path: path,
            },
            config: Self::default(),
        })
    }

    pub fn from_toml_str(contents: &str) -> Result<Self, toml::de::Error> {
        let config = toml::from_str::<Self>(contents)?;
        config
            .validate()
            .map_err(<toml::de::Error as SerdeDeError>::custom)
    }

    pub fn save(&self) -> Result<PathBuf, ConfigSaveError> {
        self.save_to_path(self.default_config_path_for_save()?)
    }

    pub fn save_to_path(&self, path: impl Into<PathBuf>) -> Result<PathBuf, ConfigSaveError> {
        let path = path.into();
        self.clone()
            .validate()
            .map_err(ConfigSaveError::Validation)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigSaveError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let contents =
            toml::to_string_pretty(self).map_err(|source| ConfigSaveError::Serialize { source })?;
        fs::write(&path, contents).map_err(|source| ConfigSaveError::Write {
            path: path.clone(),
            source,
        })?;

        Ok(path)
    }

    pub fn default_config_path() -> Result<PathBuf, ConfigError> {
        env::current_dir()
            .map(|cwd| cwd.join(DEFAULT_CONFIG_FILE_NAME))
            .map_err(|source| ConfigError::CurrentDirectory { source })
    }

    fn default_config_path_for_save(&self) -> Result<PathBuf, ConfigSaveError> {
        env::current_dir()
            .map(|cwd| cwd.join(DEFAULT_CONFIG_FILE_NAME))
            .map_err(|source| ConfigSaveError::CurrentDirectory { source })
    }

    fn validate(self) -> Result<Self, &'static str> {
        if self.game.timezone.trim().is_empty() {
            return Err("game.timezone must not be empty");
        }

        if self
            .adb
            .executable
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err("adb.executable must not be empty when provided");
        }

        if self.logging.directory.as_os_str().is_empty() {
            return Err("logging.directory must not be empty");
        }

        if self.logging.file_name.trim().is_empty() {
            return Err("logging.file_name must not be empty");
        }

        if self.debug.export_directory.as_os_str().is_empty() {
            return Err("debug.export_directory must not be empty");
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdbConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GameConfig {
    pub timezone: String,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            timezone: DEFAULT_GAME_TIMEZONE.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub directory: PathBuf,
    pub file_name: String,
}

impl LoggingConfig {
    pub fn resolved_file_path(&self, base_directory: &Path) -> PathBuf {
        base_directory
            .join(&self.directory)
            .join(self.file_name.as_str())
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from(DEFAULT_LOG_DIRECTORY_NAME),
            file_name: DEFAULT_LOG_FILE_NAME.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    pub export_artifacts: bool,
    pub export_directory: PathBuf,
}

impl DebugConfig {
    pub fn resolved_export_directory(&self, base_directory: &Path) -> PathBuf {
        base_directory.join(&self.export_directory)
    }
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            export_artifacts: false,
            export_directory: PathBuf::from(DEFAULT_DEBUG_EXPORT_DIRECTORY_NAME),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub source: ConfigSource,
    pub config: AppConfig,
}

impl LoadedConfig {
    pub fn save_path(&self) -> &Path {
        match &self.source {
            ConfigSource::Defaults { expected_path } => expected_path,
            ConfigSource::File(path) => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Defaults { expected_path: PathBuf },
    File(PathBuf),
}

impl ConfigSource {
    pub fn describe(&self) -> String {
        match self {
            Self::Defaults { expected_path } => format!(
                "defaults (no config file found at {})",
                display_path(expected_path)
            ),
            Self::File(path) => format!("file {}", display_path(path)),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to determine current working directory: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("failed to read config file `{path}`: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to parse config file `{path}`: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

#[derive(Debug, Error)]
pub enum ConfigSaveError {
    #[error("failed to determine current working directory: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("invalid config data: {0}")]
    Validation(&'static str),
    #[error("failed to create config directory `{path}`: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("failed to serialize config to TOML: {source}")]
    Serialize { source: toml::ser::Error },
    #[error("failed to write config file `{path}`: {source}")]
    Write { path: PathBuf, source: io::Error },
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::AdbConfig;
    use super::AppConfig;
    use super::ConfigSource;
    use super::DEFAULT_DEBUG_EXPORT_DIRECTORY_NAME;
    use super::DEFAULT_GAME_TIMEZONE;
    use super::DEFAULT_LOG_DIRECTORY_NAME;
    use super::DEFAULT_LOG_FILE_NAME;
    use super::DebugConfig;
    use super::GameConfig;
    use super::LoggingConfig;

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_uses_expected_defaults() {
        let config = AppConfig::default();
        assert_eq!(config.game.timezone, DEFAULT_GAME_TIMEZONE);
        assert_eq!(config.adb.executable, None);
        assert_eq!(
            config.logging.directory,
            PathBuf::from(DEFAULT_LOG_DIRECTORY_NAME)
        );
        assert_eq!(config.logging.file_name, DEFAULT_LOG_FILE_NAME);
        assert!(!config.debug.export_artifacts);
        assert_eq!(
            config.debug.export_directory,
            PathBuf::from(DEFAULT_DEBUG_EXPORT_DIRECTORY_NAME)
        );
    }

    #[test]
    fn parser_merges_defaults_with_partial_toml() {
        let config = AppConfig::from_toml_str("[adb]\nexecutable = 'C:/tools/adb.exe'\n").unwrap();

        assert_eq!(
            config.adb.executable,
            Some(PathBuf::from("C:/tools/adb.exe"))
        );
        assert_eq!(config.game.timezone, DEFAULT_GAME_TIMEZONE);
        assert_eq!(config.logging.file_name, DEFAULT_LOG_FILE_NAME);
    }

    #[test]
    fn parser_rejects_blank_timezone() {
        let error = AppConfig::from_toml_str("[game]\ntimezone = '   '\n").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("game.timezone must not be empty")
        );
    }

    #[test]
    fn load_or_default_uses_defaults_when_file_is_missing() {
        let path = unique_test_path("missing-config").join("ArkAgent.toml");
        let loaded = AppConfig::load_or_default_from(&path).unwrap();

        assert_eq!(loaded.config, AppConfig::default());
        assert_eq!(
            loaded.source,
            ConfigSource::Defaults {
                expected_path: path
            }
        );
    }

    #[test]
    fn load_from_path_reads_toml_document() {
        let dir = unique_test_path("config-file");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ArkAgent.toml");
        fs::write(
            &path,
            "[adb]\nexecutable = 'C:/platform-tools/adb.exe'\n\n[game]\ntimezone = 'UTC'\n",
        )
        .unwrap();

        let loaded = AppConfig::load_from_path(&path).unwrap();

        assert_eq!(loaded.config.game.timezone, "UTC");
        assert_eq!(
            loaded.config.adb.executable,
            Some(PathBuf::from("C:/platform-tools/adb.exe"))
        );
        assert_eq!(loaded.source, ConfigSource::File(path.clone()));

        fs::remove_file(path).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_to_path_writes_round_trippable_toml() {
        let dir = unique_test_path("save-config");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ArkAgent.toml");
        let config = AppConfig {
            adb: AdbConfig {
                executable: Some(PathBuf::from("C:/tools/adb.exe")),
            },
            game: GameConfig {
                timezone: "UTC".to_string(),
            },
            logging: LoggingConfig {
                directory: PathBuf::from("runtime-logs"),
                file_name: "desktop.log".to_string(),
            },
            debug: DebugConfig {
                export_artifacts: true,
                export_directory: PathBuf::from("captures"),
            },
        };

        let saved_path = config.save_to_path(&path).unwrap();
        let loaded = AppConfig::load_from_path(&saved_path).unwrap();

        assert_eq!(loaded.config, config);

        fs::remove_file(path).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("arkagent-{label}-{}-{nanos}", std::process::id()))
    }
}
