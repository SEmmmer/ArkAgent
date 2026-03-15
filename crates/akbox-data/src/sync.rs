use std::path::Path;

use crate::penguin::PenguinClient;
use crate::penguin::PenguinClientError;
use crate::prts::PrtsClient;
use crate::prts::PrtsClientError;
use crate::repository::AlertUpsert;
use crate::repository::AppRepository;
use crate::repository::PenguinMatrixUpsert;
use crate::repository::RawSourceCacheUpsert;
use thiserror::Error;

pub const PRTS_SITEINFO_SOURCE_ID: &str = "prts.siteinfo.general";
pub const PRTS_SITEINFO_CACHE_KEY: &str = "prts:siteinfo:general";
pub const PENGUIN_MATRIX_SOURCE_ID: &str = "penguin.matrix.cn";
pub const PENGUIN_MATRIX_CACHE_KEY: &str = "penguin:matrix:cn";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsSiteInfoOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPenguinMatrixOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
}

pub fn sync_prts_site_info(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    _base_directory: &Path,
) -> Result<SyncPrtsSiteInfoOutcome, SyncPrtsError> {
    repository
        .record_sync_attempt(PRTS_SITEINFO_SOURCE_ID)
        .map_err(SyncPrtsError::Repository)?;

    let site_info = match client.fetch_site_info() {
        Ok(site_info) => site_info,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_SITEINFO_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_SITEINFO_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsError::Repository)?;
            return Err(SyncPrtsError::Client(error));
        }
    };

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_SITEINFO_CACHE_KEY,
            source_name: "prts",
            revision: Some(site_info.server_time.as_str()),
            content_type: site_info.content_type.as_str(),
            payload: site_info.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsError::Repository)?;
    repository
        .record_sync_success(
            PRTS_SITEINFO_SOURCE_ID,
            Some(site_info.server_time.as_str()),
        )
        .map_err(SyncPrtsError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PRTS_SITEINFO_SOURCE_ID))
        .map_err(SyncPrtsError::Repository)?;

    Ok(SyncPrtsSiteInfoOutcome {
        source_id: PRTS_SITEINFO_SOURCE_ID.to_string(),
        cache_key: PRTS_SITEINFO_CACHE_KEY.to_string(),
        revision: site_info.server_time,
        cache_size_bytes: site_info.raw_body.len(),
    })
}

pub fn sync_penguin_matrix(
    repository: &AppRepository<'_>,
    client: &PenguinClient,
) -> Result<SyncPenguinMatrixOutcome, SyncPenguinError> {
    repository
        .record_sync_attempt(PENGUIN_MATRIX_SOURCE_ID)
        .map_err(SyncPenguinError::Repository)?;

    let matrix = match client.fetch_cn_matrix() {
        Ok(matrix) => matrix,
        Err(error) => {
            repository
                .record_sync_failure(PENGUIN_MATRIX_SOURCE_ID, &error.to_string())
                .map_err(SyncPenguinError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PENGUIN_MATRIX_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "Penguin 同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPenguinError::Repository)?;
            return Err(SyncPenguinError::Client(error));
        }
    };

    let revision = matrix_revision(&matrix.rows);

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PENGUIN_MATRIX_CACHE_KEY,
            source_name: "penguin",
            revision: Some(revision.as_str()),
            content_type: matrix.content_type.as_str(),
            payload: matrix.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPenguinError::Repository)?;

    let upserts = matrix
        .rows
        .iter()
        .map(|row| PenguinMatrixUpsert {
            matrix_id: format!(
                "{}:{}:{}:{}",
                row.stage_id,
                row.item_id,
                row.start,
                row.end.unwrap_or(0)
            ),
            stage_id: row.stage_id.clone(),
            item_id: row.item_id.clone(),
            sample_count: row.times,
            drop_count: row.quantity,
            window_start_at: Some(row.start.to_string()),
            window_end_at: row.end.map(|value| value.to_string()),
            raw_json: serde_json::to_value(row).expect("penguin row should be serializable"),
        })
        .collect::<Vec<_>>();

    repository
        .replace_penguin_matrix(&upserts)
        .map_err(SyncPenguinError::Repository)?;
    repository
        .record_sync_success(PENGUIN_MATRIX_SOURCE_ID, Some(revision.as_str()))
        .map_err(SyncPenguinError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PENGUIN_MATRIX_SOURCE_ID))
        .map_err(SyncPenguinError::Repository)?;

    Ok(SyncPenguinMatrixOutcome {
        source_id: PENGUIN_MATRIX_SOURCE_ID.to_string(),
        cache_key: PENGUIN_MATRIX_CACHE_KEY.to_string(),
        revision,
        cache_size_bytes: matrix.raw_body.len(),
        row_count: matrix.rows.len(),
    })
}

#[derive(Debug, Error)]
pub enum SyncPrtsError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

#[derive(Debug, Error)]
pub enum SyncPenguinError {
    #[error(transparent)]
    Client(#[from] PenguinClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

fn sync_failure_alert_id(source_id: &str) -> String {
    format!("sync-failure:{source_id}")
}

fn matrix_revision(rows: &[crate::penguin::PenguinMatrixRow]) -> String {
    rows.iter()
        .map(|row| row.end.unwrap_or(row.start))
        .max()
        .unwrap_or(0)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::PENGUIN_MATRIX_CACHE_KEY;
    use super::PENGUIN_MATRIX_SOURCE_ID;
    use super::PRTS_SITEINFO_CACHE_KEY;
    use super::PRTS_SITEINFO_SOURCE_ID;
    use super::sync_penguin_matrix;
    use super::sync_prts_site_info;
    use crate::database::AppDatabase;
    use crate::database::default_database_path;
    use crate::penguin::PenguinClient;
    use crate::prts::PrtsClient;
    use crate::repository::AppRepository;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sync_prts_site_info_writes_cache_and_sync_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"query":{"general":{"sitename":"PRTS","generator":"MediaWiki 1.43.5","base":"https://prts.wiki/w/首页","server":"//prts.wiki","time":"2026-03-16T01:00:00Z"}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("sync");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_prts_site_info(&repository, &client, &base_directory).unwrap();

            assert_eq!(outcome.source_id, PRTS_SITEINFO_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_SITEINFO_CACHE_KEY);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type, length(payload)
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_SITEINFO_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "2026-03-16T01:00:00Z");
        assert!(cache_row.2.contains("application/json"));
        assert!(cache_row.3 > 0);

        let sync_row = database
            .connection()
            .query_row(
                "SELECT status, cursor_value, last_error FROM sync_source_state WHERE source_id = ?1",
                [PRTS_SITEINFO_SOURCE_ID],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(sync_row.0, "succeeded");
        assert_eq!(sync_row.1, "2026-03-16T01:00:00Z");
        assert_eq!(sync_row.2, None);

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_failure_writes_failed_state_and_alert() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"error":"internal"}"#;
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("prts-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_site_info(&repository, &client, &base_directory).unwrap_err();
            assert!(error.to_string().contains("unexpected HTTP status"));
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_SITEINFO_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{PRTS_SITEINFO_SOURCE_ID}")],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            alert,
            (
                "sync_failure".to_string(),
                "error".to_string(),
                "active".to_string(),
            )
        );

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_penguin_matrix_writes_cache_and_drop_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"matrix":[{"stageId":"main_01-07","itemId":"30011","times":100,"quantity":31,"stdDev":0.42,"start":1744012800000,"end":null},{"stageId":"main_01-07","itemId":"30012","times":100,"quantity":52,"stdDev":0.49,"start":1744012800000,"end":null}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("penguin-sync");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PenguinClient::with_matrix_url(format!("http://{address}/matrix")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_penguin_matrix(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PENGUIN_MATRIX_SOURCE_ID);
            assert_eq!(outcome.cache_key, PENGUIN_MATRIX_CACHE_KEY);
            assert_eq!(outcome.row_count, 2);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type FROM raw_source_cache WHERE cache_key = ?1",
                [PENGUIN_MATRIX_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "penguin");
        assert_eq!(cache_row.1, "1744012800000");
        assert!(cache_row.2.contains("application/json"));

        let matrix_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_drop_matrix", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(matrix_count, 2);

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PENGUIN_MATRIX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-sync-{label}-{}-{nanos}",
            std::process::id()
        ))
    }
}
