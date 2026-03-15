use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use thiserror::Error;

use crate::config::AppConfig;

const SAMPLE_SCREENSHOT_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5, 0x1C, 0x0C,
    0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xFF, 0x1F, 0x00,
    0x03, 0x03, 0x02, 0x00, 0xEE, 0xD9, 0x94, 0x2F, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugArtifactExporter {
    enabled: bool,
    export_directory: PathBuf,
}

impl DebugArtifactExporter {
    pub fn from_config(config: &AppConfig, base_directory: &Path) -> Self {
        Self {
            enabled: config.debug.export_artifacts,
            export_directory: config.debug.resolved_export_directory(base_directory),
        }
    }

    pub fn export_directory(&self) -> &Path {
        &self.export_directory
    }

    pub fn export_bytes(
        &self,
        kind: DebugArtifactKind,
        stem: &str,
        extension: &str,
        bytes: &[u8],
    ) -> Result<DebugArtifactExportOutcome, DebugArtifactError> {
        if !self.enabled {
            return Ok(DebugArtifactExportOutcome::Disabled {
                directory: self.export_directory.clone(),
            });
        }

        fs::create_dir_all(&self.export_directory).map_err(|source| {
            DebugArtifactError::CreateDirectory {
                path: self.export_directory.clone(),
                source,
            }
        })?;

        let file_name = format!(
            "{}-{}-{}.{}",
            unix_timestamp_millis()?,
            kind.file_prefix(),
            sanitize_file_component(stem),
            normalize_extension(extension)
        );
        let path = self.export_directory.join(file_name);
        fs::write(&path, bytes).map_err(|source| DebugArtifactError::Write {
            path: path.clone(),
            source,
        })?;

        Ok(DebugArtifactExportOutcome::Exported(DebugArtifactFile {
            kind,
            path,
            bytes_written: bytes.len(),
        }))
    }

    pub fn export_json<T: Serialize>(
        &self,
        kind: DebugArtifactKind,
        stem: &str,
        value: &T,
    ) -> Result<DebugArtifactExportOutcome, DebugArtifactError> {
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|source| DebugArtifactError::SerializeJson { source })?;
        self.export_bytes(kind, stem, "json", &bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugArtifactKind {
    Screenshot,
    RecognitionResult,
}

impl DebugArtifactKind {
    fn file_prefix(self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::RecognitionResult => "recognition",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugArtifactExportOutcome {
    Disabled { directory: PathBuf },
    Exported(DebugArtifactFile),
}

impl DebugArtifactExportOutcome {
    pub fn exported_file(&self) -> Option<&DebugArtifactFile> {
        match self {
            Self::Disabled { .. } => None,
            Self::Exported(file) => Some(file),
        }
    }

    pub fn directory(&self) -> &Path {
        match self {
            Self::Disabled { directory } => directory,
            Self::Exported(file) => file
                .path
                .parent()
                .expect("exported artifact path must have a parent directory"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugArtifactFile {
    pub kind: DebugArtifactKind,
    pub path: PathBuf,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleDebugArtifactBundle {
    pub directory: PathBuf,
    pub screenshot: DebugArtifactExportOutcome,
    pub recognition: DebugArtifactExportOutcome,
}

pub fn export_sample_debug_bundle(
    config: &AppConfig,
    base_directory: &Path,
    source: &str,
) -> Result<SampleDebugArtifactBundle, DebugArtifactError> {
    let exporter = DebugArtifactExporter::from_config(config, base_directory);
    let recognition = SampleRecognitionArtifact::new(source, unix_timestamp_millis()?);

    let screenshot = exporter.export_bytes(
        DebugArtifactKind::Screenshot,
        source,
        "png",
        SAMPLE_SCREENSHOT_PNG,
    )?;
    let recognition =
        exporter.export_json(DebugArtifactKind::RecognitionResult, source, &recognition)?;

    Ok(SampleDebugArtifactBundle {
        directory: exporter.export_directory().to_path_buf(),
        screenshot,
        recognition,
    })
}

#[derive(Debug, Serialize)]
struct SampleRecognitionArtifact {
    schema_version: u8,
    source: String,
    page_id: &'static str,
    captured_at_unix_ms: u128,
    fields: Vec<SampleRecognitionField>,
}

impl SampleRecognitionArtifact {
    fn new(source: &str, captured_at_unix_ms: u128) -> Self {
        Self {
            schema_version: 1,
            source: source.to_string(),
            page_id: "settings",
            captured_at_unix_ms,
            fields: vec![
                SampleRecognitionField {
                    field_id: "adb_executable".to_string(),
                    value: "C:/MuMu/adb.exe".to_string(),
                    confidence: 1.0,
                },
                SampleRecognitionField {
                    field_id: "game_timezone".to_string(),
                    value: "Asia/Shanghai".to_string(),
                    confidence: 0.99,
                },
            ],
        }
    }
}

#[derive(Debug, Serialize)]
struct SampleRecognitionField {
    field_id: String,
    value: String,
    confidence: f32,
}

#[derive(Debug, Error)]
pub enum DebugArtifactError {
    #[error("failed to determine current timestamp: {source}")]
    CurrentTimestamp { source: std::time::SystemTimeError },
    #[error("failed to create debug export directory `{path}`: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("failed to serialize debug artifact to JSON: {source}")]
    SerializeJson { source: serde_json::Error },
    #[error("failed to write debug artifact `{path}`: {source}")]
    Write { path: PathBuf, source: io::Error },
}

fn unix_timestamp_millis() -> Result<u128, DebugArtifactError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|source| DebugArtifactError::CurrentTimestamp { source })
}

fn sanitize_file_component(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();

    let collapsed = sanitized.trim_matches('-').to_string();
    if collapsed.is_empty() {
        "artifact".to_string()
    } else {
        collapsed
    }
}

fn normalize_extension(extension: &str) -> &str {
    let trimmed = extension.trim().trim_start_matches('.');
    if trimmed.is_empty() { "bin" } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::DebugArtifactExportOutcome;
    use super::SampleDebugArtifactBundle;
    use super::export_sample_debug_bundle;
    use crate::config::AppConfig;
    use crate::config::DebugConfig;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sample_bundle_is_skipped_when_export_is_disabled() {
        let base_directory = unique_test_path("disabled");
        let bundle =
            export_sample_debug_bundle(&AppConfig::default(), &base_directory, "desktop").unwrap();

        assert!(matches!(
            bundle.screenshot,
            DebugArtifactExportOutcome::Disabled { .. }
        ));
        assert!(matches!(
            bundle.recognition,
            DebugArtifactExportOutcome::Disabled { .. }
        ));
        assert!(!bundle.directory.exists());
    }

    #[test]
    fn sample_bundle_writes_png_and_json_when_enabled() {
        let base_directory = unique_test_path("enabled");
        fs::create_dir_all(&base_directory).unwrap();

        let config = AppConfig {
            debug: DebugConfig {
                export_artifacts: true,
                export_directory: PathBuf::from("captures"),
            },
            ..AppConfig::default()
        };

        let bundle = export_sample_debug_bundle(&config, &base_directory, "desktop").unwrap();

        assert_exported_artifact(&bundle, "png", "desktop");
        assert_exported_json(&bundle, "desktop");

        fs::remove_dir_all(base_directory).unwrap();
    }

    fn assert_exported_artifact(bundle: &SampleDebugArtifactBundle, extension: &str, stem: &str) {
        let exported = bundle.screenshot.exported_file().unwrap();
        let file_name = exported.path.file_name().unwrap().to_string_lossy();

        assert_eq!(exported.kind, super::DebugArtifactKind::Screenshot);
        assert!(exported.path.is_file());
        assert_eq!(
            exported.path.extension().unwrap().to_string_lossy(),
            extension
        );
        assert!(file_name.contains(stem));
    }

    fn assert_exported_json(bundle: &SampleDebugArtifactBundle, source: &str) {
        let exported = bundle.recognition.exported_file().unwrap();
        let document = fs::read_to_string(&exported.path).unwrap();
        let json = serde_json::from_str::<Value>(&document).unwrap();

        assert_eq!(exported.kind, super::DebugArtifactKind::RecognitionResult);
        assert_eq!(json["source"], source);
        assert_eq!(json["page_id"], "settings");
        assert_eq!(json["fields"][0]["field_id"], "adb_executable");
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-debug-artifact-{label}-{}-{nanos}",
            std::process::id()
        ))
    }
}
