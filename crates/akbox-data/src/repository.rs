use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::json;
use thiserror::Error;

pub struct AppRepository<'connection> {
    connection: &'connection Connection,
}

impl<'connection> AppRepository<'connection> {
    pub fn new(connection: &'connection Connection) -> Self {
        Self { connection }
    }

    pub fn set_app_meta(&self, key: &str, value: &str) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO app_meta (meta_key, meta_value, updated_at)
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(meta_key) DO UPDATE
                 SET meta_value = excluded.meta_value,
                     updated_at = excluded.updated_at",
                params![key, value],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn get_app_meta(&self, key: &str) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT meta_value FROM app_meta WHERE meta_key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn append_audit_log(&self, entry: &AuditLogEntry<'_>) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO audit_log (
                    audit_id,
                    entity_type,
                    entity_id,
                    action,
                    summary,
                    payload_json,
                    source,
                    created_at
                ) VALUES (
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    ?5,
                    ?6,
                    ?7,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                )",
                params![
                    entry.audit_id,
                    entry.entity_type,
                    entry.entity_id,
                    entry.action,
                    entry.summary,
                    entry.payload_json,
                    entry.source,
                ],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn upsert_alert(&self, entry: &AlertUpsert<'_>) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO alert (
                    alert_id,
                    alert_type,
                    severity,
                    title,
                    message,
                    status,
                    trigger_at,
                    resolved_at,
                    payload_json
                ) VALUES (
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    ?5,
                    ?6,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    NULL,
                    ?7
                )
                ON CONFLICT(alert_id) DO UPDATE
                SET alert_type = excluded.alert_type,
                    severity = excluded.severity,
                    title = excluded.title,
                    message = excluded.message,
                    status = excluded.status,
                    trigger_at = excluded.trigger_at,
                    resolved_at = NULL,
                    payload_json = excluded.payload_json",
                params![
                    entry.alert_id,
                    entry.alert_type,
                    entry.severity,
                    entry.title,
                    entry.message,
                    entry.status,
                    entry.payload_json,
                ],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn resolve_alert(&self, alert_id: &str) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "UPDATE alert
                 SET status = 'resolved',
                     resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE alert_id = ?1",
                params![alert_id],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn upsert_raw_source_cache(
        &self,
        entry: &RawSourceCacheUpsert<'_>,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO raw_source_cache (
                    cache_key,
                    source_name,
                    revision,
                    content_type,
                    payload,
                    fetched_at,
                    expires_at
                ) VALUES (
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    ?5,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    ?6
                )
                ON CONFLICT(cache_key) DO UPDATE
                SET source_name = excluded.source_name,
                    revision = excluded.revision,
                    content_type = excluded.content_type,
                    payload = excluded.payload,
                    fetched_at = excluded.fetched_at,
                    expires_at = excluded.expires_at",
                params![
                    entry.cache_key,
                    entry.source_name,
                    entry.revision,
                    entry.content_type,
                    entry.payload,
                    entry.expires_at,
                ],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn record_sync_attempt(&self, source_id: &str) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO sync_source_state (
                    source_id,
                    status,
                    last_attempt_at,
                    last_success_at,
                    cursor_value,
                    last_error
                ) VALUES (
                    ?1,
                    'running',
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    NULL,
                    NULL,
                    NULL
                )
                ON CONFLICT(source_id) DO UPDATE
                SET status = 'running',
                    last_attempt_at = excluded.last_attempt_at,
                    last_error = NULL",
                params![source_id],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn record_sync_success(
        &self,
        source_id: &str,
        cursor_value: Option<&str>,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO sync_source_state (
                    source_id,
                    status,
                    last_attempt_at,
                    last_success_at,
                    cursor_value,
                    last_error
                ) VALUES (
                    ?1,
                    'succeeded',
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    ?2,
                    NULL
                )
                ON CONFLICT(source_id) DO UPDATE
                SET status = 'succeeded',
                    last_attempt_at = excluded.last_attempt_at,
                    last_success_at = excluded.last_success_at,
                    cursor_value = excluded.cursor_value,
                    last_error = NULL",
                params![source_id, cursor_value],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn record_sync_failure(
        &self,
        source_id: &str,
        last_error: &str,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO sync_source_state (
                    source_id,
                    status,
                    last_attempt_at,
                    last_success_at,
                    cursor_value,
                    last_error
                ) VALUES (
                    ?1,
                    'failed',
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    NULL,
                    NULL,
                    ?2
                )
                ON CONFLICT(source_id) DO UPDATE
                SET status = 'failed',
                    last_attempt_at = excluded.last_attempt_at,
                    last_error = excluded.last_error",
                params![source_id, last_error],
            )
            .map(|_| ())
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn get_sync_source_state(
        &self,
        source_id: &str,
    ) -> Result<Option<SyncSourceStateRecord>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT source_id, status, last_attempt_at, last_success_at, cursor_value, last_error
                 FROM sync_source_state
                 WHERE source_id = ?1",
                params![source_id],
                |row| {
                    Ok(SyncSourceStateRecord {
                        source_id: row.get(0)?,
                        status: row.get(1)?,
                        last_attempt_at: row.get(2)?,
                        last_success_at: row.get(3)?,
                        cursor_value: row.get(4)?,
                        last_error: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn get_raw_source_cache_summary(
        &self,
        cache_key: &str,
    ) -> Result<Option<RawSourceCacheSummary>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT cache_key, source_name, revision, content_type, length(payload), fetched_at, expires_at
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                params![cache_key],
                |row| {
                    Ok(RawSourceCacheSummary {
                        cache_key: row.get(0)?,
                        source_name: row.get(1)?,
                        revision: row.get(2)?,
                        content_type: row.get(3)?,
                        payload_bytes: row.get(4)?,
                        fetched_at: row.get(5)?,
                        expires_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn count_external_drop_matrix(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM external_drop_matrix", [], |row| {
                row.get(0)
            })
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn list_external_drop_matrix(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalDropMatrixRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT stage_id, item_id, sample_count, drop_count, window_start_at, window_end_at
                 FROM external_drop_matrix
                 ORDER BY sample_count DESC, stage_id ASC, item_id ASC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(ExternalDropMatrixRecord {
                    stage_id: row.get(0)?,
                    item_id: row.get(1)?,
                    sample_count: row.get(2)?,
                    drop_count: row.get(3)?,
                    window_start_at: row.get(4)?,
                    window_end_at: row.get(5)?,
                })
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn replace_penguin_matrix(
        &self,
        entries: &[PenguinMatrixUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        transaction
            .execute("DELETE FROM external_drop_matrix", [])
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut upsert_stage = transaction
            .prepare(
                "INSERT INTO external_stage_def (stage_id, zone_id, code, is_open, raw_json, updated_at)
                 VALUES (?1, NULL, ?1, 1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(stage_id) DO UPDATE
                 SET code = excluded.code,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;
        let mut upsert_item = transaction
            .prepare(
                "INSERT INTO external_item_def (item_id, name_zh, item_type, rarity, raw_json, updated_at)
                 VALUES (?1, ?1, 'unknown', NULL, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(item_id) DO UPDATE
                 SET name_zh = excluded.name_zh,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;
        let mut insert_matrix = transaction
            .prepare(
                "INSERT INTO external_drop_matrix (
                    matrix_id,
                    stage_id,
                    item_id,
                    sample_count,
                    drop_count,
                    window_start_at,
                    window_end_at,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let stage_stub = json!({
                "stage_id": entry.stage_id,
                "source": "penguin",
            });
            let item_stub = json!({
                "item_id": entry.item_id,
                "source": "penguin",
            });
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;
            let stage_stub_json = serde_json::to_string(&stage_stub)
                .map_err(|source| RepositoryError::SerializeJson { source })?;
            let item_stub_json = serde_json::to_string(&item_stub)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            upsert_stage
                .execute(params![entry.stage_id, stage_stub_json])
                .map_err(|source| RepositoryError::Sqlite { source })?;
            upsert_item
                .execute(params![entry.item_id, item_stub_json])
                .map_err(|source| RepositoryError::Sqlite { source })?;
            insert_matrix
                .execute(params![
                    entry.matrix_id,
                    entry.stage_id,
                    entry.item_id,
                    entry.sample_count,
                    entry.drop_count,
                    entry.window_start_at,
                    entry.window_end_at,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(insert_matrix);
        drop(upsert_item);
        drop(upsert_stage);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }
}

pub struct AuditLogEntry<'a> {
    pub audit_id: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<&'a str>,
    pub action: &'a str,
    pub summary: &'a str,
    pub payload_json: Option<&'a str>,
    pub source: &'a str,
}

pub struct RawSourceCacheUpsert<'a> {
    pub cache_key: &'a str,
    pub source_name: &'a str,
    pub revision: Option<&'a str>,
    pub content_type: &'a str,
    pub payload: &'a [u8],
    pub expires_at: Option<&'a str>,
}

pub struct AlertUpsert<'a> {
    pub alert_id: &'a str,
    pub alert_type: &'a str,
    pub severity: &'a str,
    pub title: &'a str,
    pub message: &'a str,
    pub status: &'a str,
    pub payload_json: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSourceStateRecord {
    pub source_id: String,
    pub status: String,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub cursor_value: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSourceCacheSummary {
    pub cache_key: String,
    pub source_name: String,
    pub revision: Option<String>,
    pub content_type: String,
    pub payload_bytes: i64,
    pub fetched_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalDropMatrixRecord {
    pub stage_id: String,
    pub item_id: String,
    pub sample_count: i64,
    pub drop_count: i64,
    pub window_start_at: Option<String>,
    pub window_end_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinMatrixUpsert {
    pub matrix_id: String,
    pub stage_id: String,
    pub item_id: String,
    pub sample_count: i64,
    pub drop_count: i64,
    pub window_start_at: Option<String>,
    pub window_end_at: Option<String>,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("sqlite operation failed: {source}")]
    Sqlite { source: rusqlite::Error },
    #[error("failed to serialize repository JSON payload: {source}")]
    SerializeJson { source: serde_json::Error },
}

#[cfg(test)]
mod tests {
    use super::AppRepository;
    use super::AuditLogEntry;
    use crate::database::AppDatabase;
    use crate::database::default_database_path;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn repository_can_upsert_app_meta() {
        let base_directory = unique_test_path("meta");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository.set_app_meta("schema_version", "1").unwrap();
            repository.set_app_meta("schema_version", "2").unwrap();

            assert_eq!(
                repository.get_app_meta("schema_version").unwrap(),
                Some("2".to_string())
            );
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_append_audit_log() {
        let base_directory = unique_test_path("audit");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .append_audit_log(&AuditLogEntry {
                    audit_id: "audit-001",
                    entity_type: "inventory_item",
                    entity_id: Some("item-001"),
                    action: "upsert",
                    summary: "updated test item quantity",
                    payload_json: Some(r#"{"quantity": 12}"#),
                    source: "unit-test",
                })
                .unwrap();
        }

        let stored = database
            .connection()
            .query_row(
                "SELECT entity_type, entity_id, action, summary, payload_json, source
                 FROM audit_log
                 WHERE audit_id = 'audit-001'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(
            stored,
            (
                "inventory_item".to_string(),
                "item-001".to_string(),
                "upsert".to_string(),
                "updated test item quantity".to_string(),
                r#"{"quantity": 12}"#.to_string(),
                "unit-test".to_string(),
            )
        );

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-data-repo-{label}-{}-{nanos}",
            std::process::id()
        ))
    }
}
