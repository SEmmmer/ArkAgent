use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite_migration::M;
use rusqlite_migration::Migrations;
use thiserror::Error;

pub const DEFAULT_DATABASE_DIRECTORY_NAME: &str = "data";
pub const DEFAULT_DATABASE_FILE_NAME: &str = "arkagent.db";

const INITIAL_MIGRATION: &str = include_str!("../../../migrations/0001_initial.sql");

pub struct AppDatabase {
    path: PathBuf,
    connection: Connection,
}

impl AppDatabase {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, DatabaseError> {
        let path = path.into();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| DatabaseError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let mut connection = Connection::open(&path).map_err(|source| DatabaseError::Open {
            path: path.clone(),
            source,
        })?;
        configure_connection(&connection)?;
        migrations()
            .to_latest(&mut connection)
            .map_err(|source| DatabaseError::Migrate {
                path: path.clone(),
                source,
            })?;

        Ok(Self { path, connection })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

pub fn default_database_path(base_directory: &Path) -> PathBuf {
    base_directory
        .join(DEFAULT_DATABASE_DIRECTORY_NAME)
        .join(DEFAULT_DATABASE_FILE_NAME)
}

fn configure_connection(connection: &Connection) -> Result<(), DatabaseError> {
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|source| DatabaseError::Configure { source })?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|source| DatabaseError::Configure { source })?;
    Ok(())
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(INITIAL_MIGRATION)])
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("failed to create database directory `{path}`: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to open SQLite database `{path}`: {source}")]
    Open {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("failed to configure SQLite connection: {source}")]
    Configure { source: rusqlite::Error },
    #[error("failed to apply SQLite migrations to `{path}`: {source}")]
    Migrate {
        path: PathBuf,
        source: rusqlite_migration::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::AppDatabase;
    use super::default_database_path;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn open_creates_database_with_all_core_tables() {
        let base_directory = unique_test_path("tables");
        let database_path = default_database_path(&base_directory);
        let database = AppDatabase::open(&database_path).unwrap();
        let expected_tables = [
            "app_meta",
            "sync_source_state",
            "raw_source_cache",
            "external_operator_def",
            "external_operator_growth",
            "external_operator_building_skill",
            "external_item_def",
            "external_recipe",
            "external_stage_def",
            "external_drop_matrix",
            "external_event_notice",
            "inventory_snapshot",
            "inventory_item_state",
            "operator_snapshot",
            "operator_state",
            "scan_artifact",
            "recognition_review_queue",
            "resource_policy",
            "floor_profile",
            "floor_profile_member",
            "planner_run",
            "planner_recommendation",
            "base_layout_config",
            "base_shift_plan",
            "alert",
            "audit_log",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();

        let actual_tables = database
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<BTreeSet<_>>();

        for table_name in expected_tables {
            assert!(actual_tables.contains(table_name));
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn open_enables_wal_mode() {
        let base_directory = unique_test_path("wal");
        let database_path = default_database_path(&base_directory);
        let database = AppDatabase::open(&database_path).unwrap();

        let journal_mode = database
            .connection()
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .unwrap();

        assert_eq!(journal_mode.to_lowercase(), "wal");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-data-db-{label}-{}-{nanos}",
            std::process::id()
        ))
    }
}
