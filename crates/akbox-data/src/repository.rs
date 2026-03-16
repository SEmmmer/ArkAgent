use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use std::collections::HashSet;
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

    pub fn count_external_event_notices(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM external_event_notice", [], |row| {
                row.get(0)
            })
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn count_external_item_defs(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM external_item_def", [], |row| {
                row.get(0)
            })
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn count_external_operator_defs(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM external_operator_def", [], |row| {
                row.get(0)
            })
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn count_external_operator_growths(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM external_operator_growth", [], |row| {
                row.get(0)
            })
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn find_external_item_ids_by_name_zh(
        &self,
        name_zh: &str,
    ) -> Result<Vec<String>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT item_id
                 FROM external_item_def
                 WHERE name_zh = ?1
                 ORDER BY item_id ASC",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![name_zh], |row| row.get::<_, String>(0))
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn find_external_item_matches_by_name_zh(
        &self,
        name_zh: &str,
    ) -> Result<Vec<ExternalItemNameMatchRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT
                    item_id,
                    item_type,
                    rarity,
                    CAST(json_type(raw_json, '$.categories') IS NOT NULL AS INTEGER),
                    CAST(json_extract(raw_json, '$.sortId') AS TEXT),
                    CAST(json_extract(raw_json, '$.groupID') AS TEXT)
                 FROM external_item_def
                 WHERE name_zh = ?1
                 ORDER BY item_id ASC",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![name_zh], |row| {
                Ok(ExternalItemNameMatchRecord {
                    item_id: row.get(0)?,
                    item_type: row.get(1)?,
                    rarity: row.get(2)?,
                    has_prts_payload: row.get::<_, i64>(3)? != 0,
                    penguin_sort_id: row.get(4)?,
                    penguin_group_id: row.get(5)?,
                })
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn count_external_recipes(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM external_recipe", [], |row| row.get(0))
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn count_prts_stage_defs(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row(
                "SELECT COUNT(*)
                 FROM external_stage_def
                 WHERE json_type(raw_json, '$.prts.stage_id') IS NOT NULL",
                [],
                |row| row.get(0),
            )
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

    pub fn list_penguin_drop_display_records(
        &self,
    ) -> Result<Vec<PenguinDropDisplayRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT
                    matrix.stage_id,
                    stage.code,
                    CAST(json_extract(stage.raw_json, '$.stageType') AS TEXT),
                    CAST(json_extract(stage.raw_json, '$.apCost') AS INTEGER),
                    CAST(json_extract(stage.raw_json, '$.existence.CN.exist') AS INTEGER),
                    CAST(json_extract(stage.raw_json, '$.existence.CN.openTime') AS TEXT),
                    CAST(json_extract(stage.raw_json, '$.existence.CN.closeTime') AS TEXT),
                    (
                        SELECT json_extract(drop_info.value, '$.dropType')
                        FROM json_each(stage.raw_json, '$.dropInfos') AS drop_info
                        WHERE json_extract(drop_info.value, '$.itemId') = matrix.item_id
                        LIMIT 1
                    ),
                    matrix.item_id,
                    COALESCE(item.name_zh, matrix.item_id),
                    item.item_type,
                    item.rarity,
                    matrix.sample_count,
                    matrix.drop_count,
                    matrix.window_start_at,
                    matrix.window_end_at
                 FROM external_drop_matrix AS matrix
                 LEFT JOIN external_stage_def AS stage
                    ON stage.stage_id = matrix.stage_id
                 LEFT JOIN external_item_def AS item
                    ON item.item_id = matrix.item_id
                 ORDER BY matrix.sample_count DESC, matrix.stage_id ASC, matrix.item_id ASC",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map([], |row| {
                Ok(PenguinDropDisplayRecord {
                    stage_id: row.get(0)?,
                    stage_code: row.get(1)?,
                    stage_type: row.get(2)?,
                    ap_cost: row.get(3)?,
                    stage_exists: row.get::<_, Option<i64>>(4)?.is_none_or(|value| value != 0),
                    stage_open_at: row.get(5)?,
                    stage_close_at: row.get(6)?,
                    drop_type: row.get(7)?,
                    item_id: row.get(8)?,
                    item_name: row.get(9)?,
                    item_type: row.get(10)?,
                    item_rarity: row.get(11)?,
                    sample_count: row.get(12)?,
                    drop_count: row.get(13)?,
                    window_start_at: row.get(14)?,
                    window_end_at: row.get(15)?,
                })
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn list_external_event_notices(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalEventNoticeRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT notice_id, title, notice_type, published_at, start_at, end_at, source_url, confirmed
                 FROM external_event_notice
                 ORDER BY published_at DESC, notice_id DESC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(ExternalEventNoticeRecord {
                    notice_id: row.get(0)?,
                    title: row.get(1)?,
                    notice_type: row.get(2)?,
                    published_at: row.get(3)?,
                    start_at: row.get(4)?,
                    end_at: row.get(5)?,
                    source_url: row.get(6)?,
                    confirmed: row.get::<_, i64>(7)? != 0,
                })
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn list_external_item_defs(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalItemDefRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT item_id, name_zh, item_type, rarity
                 FROM external_item_def
                 ORDER BY COALESCE(rarity, -1) DESC, CAST(item_id AS INTEGER) ASC, item_id ASC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(ExternalItemDefRecord {
                    item_id: row.get(0)?,
                    name_zh: row.get(1)?,
                    item_type: row.get(2)?,
                    rarity: row.get(3)?,
                })
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn list_external_operator_defs(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalOperatorDefRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT operator_id, name_zh, rarity, profession, branch
                 FROM external_operator_def
                 ORDER BY rarity DESC, name_zh ASC, operator_id ASC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(ExternalOperatorDefRecord {
                    operator_id: row.get(0)?,
                    name_zh: row.get(1)?,
                    rarity: row.get(2)?,
                    profession: row.get(3)?,
                    branch: row.get(4)?,
                })
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        Ok(rows)
    }

    pub fn list_external_operator_growths(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalOperatorGrowthRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT
                    growth.growth_id,
                    growth.operator_id,
                    COALESCE(operator_def.name_zh, growth.operator_id),
                    growth.stage_label,
                    growth.material_slot,
                    growth.raw_json
                 FROM external_operator_growth AS growth
                 LEFT JOIN external_operator_def AS operator_def
                    ON operator_def.operator_id = growth.operator_id
                 ORDER BY
                    COALESCE(operator_def.rarity, -1) DESC,
                    COALESCE(operator_def.name_zh, growth.operator_id) ASC,
                    growth.stage_label ASC,
                    growth.material_slot ASC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .map(|row| {
                let (
                    growth_id,
                    operator_id,
                    operator_name_zh,
                    stage_label,
                    material_slot,
                    raw_json,
                ) = row.map_err(|source| RepositoryError::Sqlite { source })?;
                parse_external_operator_growth_record(
                    growth_id,
                    operator_id,
                    operator_name_zh,
                    stage_label,
                    material_slot,
                    &raw_json,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn list_external_recipes(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalRecipeRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT
                    recipe.recipe_id,
                    recipe.output_item_id,
                    COALESCE(item.name_zh, recipe.output_item_id),
                    recipe.room_type,
                    recipe.raw_json
                 FROM external_recipe AS recipe
                 LEFT JOIN external_item_def AS item
                    ON item.item_id = recipe.output_item_id
                 ORDER BY
                    CAST(json_extract(recipe.raw_json, '$.workshop_level') AS INTEGER) ASC,
                    COALESCE(item.name_zh, recipe.output_item_id) ASC,
                    recipe.recipe_id ASC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .map(|row| {
                let (recipe_id, output_item_id, output_name_zh, room_type, raw_json) =
                    row.map_err(|source| RepositoryError::Sqlite { source })?;
                parse_external_recipe_record(
                    recipe_id,
                    output_item_id,
                    output_name_zh,
                    room_type,
                    &raw_json,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn list_prts_stage_defs(
        &self,
        limit: i64,
    ) -> Result<Vec<ExternalStageDefRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT stage_id, zone_id, code, is_open, raw_json
                 FROM external_stage_def
                 WHERE json_type(raw_json, '$.prts.stage_id') IS NOT NULL
                 ORDER BY code ASC, stage_id ASC
                 LIMIT ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|source| RepositoryError::Sqlite { source })?
            .map(|row| {
                let (stage_id, zone_id, code, is_open, raw_json) =
                    row.map_err(|source| RepositoryError::Sqlite { source })?;
                parse_prts_stage_record(stage_id, zone_id, code, is_open, &raw_json)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn upsert_external_event_notices(
        &self,
        entries: &[ExternalEventNoticeUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut statement = transaction
            .prepare(
                "INSERT INTO external_event_notice (
                    notice_id,
                    title,
                    notice_type,
                    published_at,
                    start_at,
                    end_at,
                    source_url,
                    confirmed,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )
                 ON CONFLICT(notice_id) DO UPDATE
                 SET title = excluded.title,
                     notice_type = excluded.notice_type,
                     published_at = excluded.published_at,
                     start_at = excluded.start_at,
                     end_at = excluded.end_at,
                     source_url = excluded.source_url,
                     confirmed = excluded.confirmed,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            statement
                .execute(params![
                    entry.notice_id,
                    entry.title,
                    entry.notice_type,
                    entry.published_at,
                    entry.start_at,
                    entry.end_at,
                    entry.source_url,
                    if entry.confirmed { 1_i64 } else { 0_i64 },
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(statement);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn upsert_external_item_defs(
        &self,
        entries: &[ExternalItemDefUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut statement = transaction
            .prepare(
                "INSERT INTO external_item_def (
                    item_id,
                    name_zh,
                    item_type,
                    rarity,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )
                 ON CONFLICT(item_id) DO UPDATE
                 SET name_zh = excluded.name_zh,
                     item_type = excluded.item_type,
                     rarity = excluded.rarity,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            statement
                .execute(params![
                    entry.item_id,
                    entry.name_zh,
                    entry.item_type,
                    entry.rarity,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(statement);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn upsert_external_operator_defs(
        &self,
        entries: &[ExternalOperatorDefUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut statement = transaction
            .prepare(
                "INSERT INTO external_operator_def (
                    operator_id,
                    name_zh,
                    rarity,
                    profession,
                    branch,
                    server,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )
                 ON CONFLICT(operator_id) DO UPDATE
                 SET name_zh = excluded.name_zh,
                     rarity = excluded.rarity,
                     profession = excluded.profession,
                     branch = excluded.branch,
                     server = excluded.server,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            statement
                .execute(params![
                    entry.operator_id,
                    entry.name_zh,
                    entry.rarity,
                    entry.profession,
                    entry.branch,
                    entry.server,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(statement);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn replace_external_operator_defs(
        &self,
        entries: &[ExternalOperatorDefUpsert],
    ) -> Result<(), RepositoryError> {
        let expected_operator_ids = entries
            .iter()
            .map(|entry| entry.operator_id.as_str())
            .collect::<HashSet<_>>();
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut statement = transaction
            .prepare(
                "INSERT INTO external_operator_def (
                    operator_id,
                    name_zh,
                    rarity,
                    profession,
                    branch,
                    server,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )
                 ON CONFLICT(operator_id) DO UPDATE
                 SET name_zh = excluded.name_zh,
                     rarity = excluded.rarity,
                     profession = excluded.profession,
                     branch = excluded.branch,
                     server = excluded.server,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            statement
                .execute(params![
                    entry.operator_id,
                    entry.name_zh,
                    entry.rarity,
                    entry.profession,
                    entry.branch,
                    entry.server,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(statement);

        let mut stale_operator_ids = Vec::new();
        {
            let mut stale_query = transaction
                .prepare("SELECT operator_id FROM external_operator_def")
                .map_err(|source| RepositoryError::Sqlite { source })?;
            let rows = stale_query
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|source| RepositoryError::Sqlite { source })?;

            for row in rows {
                let operator_id = row.map_err(|source| RepositoryError::Sqlite { source })?;
                if !expected_operator_ids.contains(operator_id.as_str()) {
                    stale_operator_ids.push(operator_id);
                }
            }
        }

        if !stale_operator_ids.is_empty() {
            let mut delete_growth_statement = transaction
                .prepare("DELETE FROM external_operator_growth WHERE operator_id = ?1")
                .map_err(|source| RepositoryError::Sqlite { source })?;
            let mut delete_building_skill_statement = transaction
                .prepare("DELETE FROM external_operator_building_skill WHERE operator_id = ?1")
                .map_err(|source| RepositoryError::Sqlite { source })?;
            let mut delete_operator_statement = transaction
                .prepare("DELETE FROM external_operator_def WHERE operator_id = ?1")
                .map_err(|source| RepositoryError::Sqlite { source })?;

            for operator_id in &stale_operator_ids {
                delete_growth_statement
                    .execute(params![operator_id])
                    .map_err(|source| RepositoryError::Sqlite { source })?;
                delete_building_skill_statement
                    .execute(params![operator_id])
                    .map_err(|source| RepositoryError::Sqlite { source })?;
                delete_operator_statement
                    .execute(params![operator_id])
                    .map_err(|source| RepositoryError::Sqlite { source })?;
            }
        }

        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn replace_external_operator_growths(
        &self,
        entries: &[ExternalOperatorGrowthUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        transaction
            .execute("DELETE FROM external_operator_growth", [])
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut statement = transaction
            .prepare(
                "INSERT INTO external_operator_growth (
                    growth_id,
                    operator_id,
                    stage_label,
                    material_slot,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            statement
                .execute(params![
                    entry.growth_id,
                    entry.operator_id,
                    entry.stage_label,
                    entry.material_slot,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(statement);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn upsert_external_stage_defs(
        &self,
        entries: &[ExternalStageDefUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut select_existing_stage = transaction
            .prepare(
                "SELECT zone_id, is_open, raw_json
                 FROM external_stage_def
                 WHERE stage_id = ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;
        let mut upsert_stage = transaction
            .prepare(
                "INSERT INTO external_stage_def (stage_id, zone_id, code, is_open, raw_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(stage_id) DO UPDATE
                 SET zone_id = excluded.zone_id,
                     is_open = excluded.is_open,
                     code = excluded.code,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let existing =
                load_existing_stage_payload(&mut select_existing_stage, &entry.stage_id)?;
            let zone_id = entry
                .zone_id
                .as_deref()
                .or(existing.as_ref().and_then(|value| value.zone_id.as_deref()));
            let is_open = existing
                .as_ref()
                .map(|value| value.is_open)
                .unwrap_or(entry.is_open);
            let raw_json = merge_prts_stage_raw_json(
                existing.as_ref().map(|value| value.raw_json.as_str()),
                &entry.raw_json,
            )?;

            upsert_stage
                .execute(params![
                    entry.stage_id,
                    zone_id,
                    entry.code,
                    if is_open { 1_i64 } else { 0_i64 },
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(upsert_stage);
        drop(select_existing_stage);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn replace_external_recipes(
        &self,
        entries: &[ExternalRecipeUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        transaction
            .execute("DELETE FROM external_recipe", [])
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut statement = transaction
            .prepare(
                "INSERT INTO external_recipe (
                    recipe_id,
                    output_item_id,
                    room_type,
                    raw_json,
                    updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 )",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            statement
                .execute(params![
                    entry.recipe_id,
                    entry.output_item_id,
                    entry.room_type,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        drop(statement);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }

    pub fn replace_penguin_matrix(
        &self,
        entries: &[PenguinMatrixUpsert],
        stages: &[PenguinStageUpsert],
        items: &[PenguinItemUpsert],
    ) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| RepositoryError::Sqlite { source })?;

        transaction
            .execute("DELETE FROM external_drop_matrix", [])
            .map_err(|source| RepositoryError::Sqlite { source })?;

        let mut select_existing_stage = transaction
            .prepare(
                "SELECT zone_id, is_open, raw_json
                 FROM external_stage_def
                 WHERE stage_id = ?1",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;
        let mut upsert_stage = transaction
            .prepare(
                "INSERT INTO external_stage_def (stage_id, zone_id, code, is_open, raw_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(stage_id) DO UPDATE
                 SET zone_id = excluded.zone_id,
                     is_open = excluded.is_open,
                     code = excluded.code,
                     raw_json = excluded.raw_json,
                     updated_at = excluded.updated_at",
            )
            .map_err(|source| RepositoryError::Sqlite { source })?;
        let mut upsert_item = transaction
            .prepare(
                "INSERT INTO external_item_def (item_id, name_zh, item_type, rarity, raw_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(item_id) DO UPDATE
                 SET name_zh = CASE
                        WHEN external_item_def.name_zh = external_item_def.item_id
                             OR external_item_def.item_type = 'unknown'
                        THEN excluded.name_zh
                        ELSE external_item_def.name_zh
                     END,
                     item_type = CASE
                        WHEN external_item_def.item_type = 'unknown'
                        THEN excluded.item_type
                        ELSE external_item_def.item_type
                     END,
                     rarity = COALESCE(external_item_def.rarity, excluded.rarity),
                     raw_json = CASE
                        WHEN external_item_def.item_type = 'unknown'
                        THEN excluded.raw_json
                        ELSE external_item_def.raw_json
                     END,
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

        for stage in stages {
            let existing =
                load_existing_stage_payload(&mut select_existing_stage, &stage.stage_id)?;
            let raw_json = merge_penguin_stage_raw_json(
                existing.as_ref().map(|value| value.raw_json.as_str()),
                &stage.raw_json,
            )?;
            let zone_id = stage
                .zone_id
                .as_deref()
                .or(existing.as_ref().and_then(|value| value.zone_id.as_deref()));

            upsert_stage
                .execute(params![
                    stage.stage_id,
                    zone_id,
                    stage.code,
                    if stage.is_open { 1_i64 } else { 0_i64 },
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        for item in items {
            let raw_json = serde_json::to_string(&item.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

            upsert_item
                .execute(params![
                    item.item_id,
                    item.name_zh,
                    item.item_type,
                    item.rarity,
                    raw_json,
                ])
                .map_err(|source| RepositoryError::Sqlite { source })?;
        }

        for entry in entries {
            let raw_json = serde_json::to_string(&entry.raw_json)
                .map_err(|source| RepositoryError::SerializeJson { source })?;

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
        drop(select_existing_stage);
        transaction
            .commit()
            .map_err(|source| RepositoryError::Sqlite { source })
    }
}

struct ExistingStagePayload {
    zone_id: Option<String>,
    is_open: bool,
    raw_json: String,
}

fn load_existing_stage_payload(
    statement: &mut rusqlite::Statement<'_>,
    stage_id: &str,
) -> Result<Option<ExistingStagePayload>, RepositoryError> {
    statement
        .query_row(params![stage_id], |row| {
            Ok(ExistingStagePayload {
                zone_id: row.get(0)?,
                is_open: row.get::<_, i64>(1)? != 0,
                raw_json: row.get(2)?,
            })
        })
        .optional()
        .map_err(|source| RepositoryError::Sqlite { source })
}

fn merge_prts_stage_raw_json(
    existing_raw_json: Option<&str>,
    prts_raw_json: &serde_json::Value,
) -> Result<String, RepositoryError> {
    let mut root = parse_stage_root_object(existing_raw_json)?;
    root.insert("prts".to_string(), prts_raw_json.clone());

    serde_json::to_string(&serde_json::Value::Object(root))
        .map_err(|source| RepositoryError::SerializeJson { source })
}

fn merge_penguin_stage_raw_json(
    existing_raw_json: Option<&str>,
    penguin_raw_json: &serde_json::Value,
) -> Result<String, RepositoryError> {
    let mut root = match penguin_raw_json {
        serde_json::Value::Object(map) => map.clone(),
        _ => {
            let mut map = serde_json::Map::new();
            map.insert("penguin".to_string(), penguin_raw_json.clone());
            map
        }
    };

    if let Some(existing_raw_json) = existing_raw_json {
        let existing_root = serde_json::from_str::<serde_json::Value>(existing_raw_json)
            .map_err(|source| RepositoryError::SerializeJson { source })?;
        if let Some(prts_payload) = existing_root.get("prts") {
            root.insert("prts".to_string(), prts_payload.clone());
        }
    }

    serde_json::to_string(&serde_json::Value::Object(root))
        .map_err(|source| RepositoryError::SerializeJson { source })
}

fn parse_stage_root_object(
    existing_raw_json: Option<&str>,
) -> Result<serde_json::Map<String, serde_json::Value>, RepositoryError> {
    let Some(existing_raw_json) = existing_raw_json else {
        return Ok(serde_json::Map::new());
    };

    let existing_value = serde_json::from_str::<serde_json::Value>(existing_raw_json)
        .map_err(|source| RepositoryError::SerializeJson { source })?;
    Ok(existing_value.as_object().cloned().unwrap_or_default())
}

fn parse_external_operator_growth_record(
    growth_id: String,
    operator_id: String,
    operator_name_zh: String,
    stage_label: String,
    material_slot: String,
    raw_json: &str,
) -> Result<ExternalOperatorGrowthRecord, RepositoryError> {
    let raw_value = serde_json::from_str::<serde_json::Value>(raw_json)
        .map_err(|source| RepositoryError::SerializeJson { source })?;
    let material_summary = raw_value
        .get("materials")
        .and_then(serde_json::Value::as_array)
        .map(|materials| {
            materials
                .iter()
                .filter_map(|material| {
                    let item_name = material.get("item_name_zh")?.as_str()?;
                    let count = material.get("count")?.as_i64()?;
                    Some(format!("{item_name} x{count}"))
                })
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();

    Ok(ExternalOperatorGrowthRecord {
        growth_id,
        operator_id,
        operator_name_zh,
        stage_label,
        material_slot,
        material_summary,
    })
}

fn parse_prts_stage_record(
    stage_id: String,
    zone_id: Option<String>,
    code: String,
    is_open: bool,
    raw_json: &str,
) -> Result<ExternalStageDefRecord, RepositoryError> {
    let raw_value = serde_json::from_str::<serde_json::Value>(raw_json)
        .map_err(|source| RepositoryError::SerializeJson { source })?;
    let prts_payload = raw_value
        .get("prts")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let page_title = prts_payload
        .get("page_title")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let categories = prts_payload
        .get("categories")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(ExternalStageDefRecord {
        stage_id,
        zone_id,
        code,
        is_open,
        page_title,
        categories,
    })
}

fn parse_external_recipe_record(
    recipe_id: String,
    output_item_id: String,
    output_name_zh: String,
    room_type: String,
    raw_json: &str,
) -> Result<ExternalRecipeRecord, RepositoryError> {
    let raw_value = serde_json::from_str::<serde_json::Value>(raw_json)
        .map_err(|source| RepositoryError::SerializeJson { source })?;
    let workshop_level = raw_value
        .get("workshop_level")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let recipe_kind = raw_value
        .get("recipe_kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("未分类")
        .to_string();
    let ingredient_summary = raw_value
        .get("ingredients")
        .and_then(serde_json::Value::as_array)
        .map(|ingredients| {
            ingredients
                .iter()
                .filter_map(|ingredient| {
                    let item_name = ingredient.get("item_name_zh")?.as_str()?;
                    let count = ingredient
                        .get("count")
                        .and_then(serde_json::Value::as_i64)?;
                    Some(format!("{item_name} x{count}"))
                })
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();

    Ok(ExternalRecipeRecord {
        recipe_id,
        output_item_id,
        output_name_zh,
        room_type,
        recipe_kind,
        workshop_level,
        ingredient_summary,
    })
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEventNoticeRecord {
    pub notice_id: String,
    pub title: String,
    pub notice_type: String,
    pub published_at: String,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub source_url: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PenguinDropDisplayRecord {
    pub stage_id: String,
    pub stage_code: Option<String>,
    pub stage_type: Option<String>,
    pub ap_cost: Option<i64>,
    pub stage_exists: bool,
    pub stage_open_at: Option<String>,
    pub stage_close_at: Option<String>,
    pub drop_type: Option<String>,
    pub item_id: String,
    pub item_name: String,
    pub item_type: Option<String>,
    pub item_rarity: Option<i64>,
    pub sample_count: i64,
    pub drop_count: i64,
    pub window_start_at: Option<String>,
    pub window_end_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalItemDefRecord {
    pub item_id: String,
    pub name_zh: String,
    pub item_type: String,
    pub rarity: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOperatorDefRecord {
    pub operator_id: String,
    pub name_zh: String,
    pub rarity: i64,
    pub profession: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOperatorGrowthRecord {
    pub growth_id: String,
    pub operator_id: String,
    pub operator_name_zh: String,
    pub stage_label: String,
    pub material_slot: String,
    pub material_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalItemNameMatchRecord {
    pub item_id: String,
    pub item_type: String,
    pub rarity: Option<i64>,
    pub has_prts_payload: bool,
    pub penguin_sort_id: Option<String>,
    pub penguin_group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRecipeRecord {
    pub recipe_id: String,
    pub output_item_id: String,
    pub output_name_zh: String,
    pub room_type: String,
    pub recipe_kind: String,
    pub workshop_level: i64,
    pub ingredient_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalStageDefRecord {
    pub stage_id: String,
    pub zone_id: Option<String>,
    pub code: String,
    pub is_open: bool,
    pub page_title: Option<String>,
    pub categories: Vec<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinStageUpsert {
    pub stage_id: String,
    pub zone_id: Option<String>,
    pub code: String,
    pub is_open: bool,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinItemUpsert {
    pub item_id: String,
    pub name_zh: String,
    pub item_type: String,
    pub rarity: Option<i64>,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalEventNoticeUpsert {
    pub notice_id: String,
    pub title: String,
    pub notice_type: String,
    pub published_at: String,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub source_url: String,
    pub confirmed: bool,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalItemDefUpsert {
    pub item_id: String,
    pub name_zh: String,
    pub item_type: String,
    pub rarity: Option<i64>,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalOperatorDefUpsert {
    pub operator_id: String,
    pub name_zh: String,
    pub rarity: i64,
    pub profession: String,
    pub branch: Option<String>,
    pub server: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalOperatorGrowthUpsert {
    pub growth_id: String,
    pub operator_id: String,
    pub stage_label: String,
    pub material_slot: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalRecipeUpsert {
    pub recipe_id: String,
    pub output_item_id: String,
    pub room_type: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalStageDefUpsert {
    pub stage_id: String,
    pub zone_id: Option<String>,
    pub code: String,
    pub is_open: bool,
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
    use super::ExternalEventNoticeUpsert;
    use super::ExternalItemDefUpsert;
    use super::ExternalOperatorDefUpsert;
    use super::ExternalOperatorGrowthUpsert;
    use super::ExternalRecipeUpsert;
    use super::ExternalStageDefUpsert;
    use super::PenguinStageUpsert;
    use crate::database::AppDatabase;
    use crate::database::default_database_path;
    use serde_json::json;
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

    #[test]
    fn repository_can_upsert_and_list_external_event_notices() {
        let base_directory = unique_test_path("event-notice");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .upsert_external_event_notices(&[
                    ExternalEventNoticeUpsert {
                        notice_id: "notice-001".to_string(),
                        title: "活动预告".to_string(),
                        notice_type: "activity".to_string(),
                        published_at: "2026-03-10T12:00:00+08:00".to_string(),
                        start_at: Some("2026-03-14T16:00:00+08:00".to_string()),
                        end_at: Some("2026-04-25T03:59:00+08:00".to_string()),
                        source_url: "https://ak.hypergryph.com/news/notice-001".to_string(),
                        confirmed: true,
                        raw_json: json!({"title": "活动预告"}),
                    },
                    ExternalEventNoticeUpsert {
                        notice_id: "notice-002".to_string(),
                        title: "停机维护公告".to_string(),
                        notice_type: "notice".to_string(),
                        published_at: "2026-03-11T16:00:00+08:00".to_string(),
                        start_at: Some("2026-03-14T16:00:00+08:00".to_string()),
                        end_at: Some("2026-03-14T17:00:00+08:00".to_string()),
                        source_url: "https://ak.hypergryph.com/news/notice-002".to_string(),
                        confirmed: true,
                        raw_json: json!({"title": "停机维护公告"}),
                    },
                ])
                .unwrap();

            assert_eq!(repository.count_external_event_notices().unwrap(), 2);

            let notices = repository.list_external_event_notices(4).unwrap();
            assert_eq!(notices.len(), 2);
            assert_eq!(notices[0].notice_id, "notice-002");
            assert_eq!(notices[1].notice_id, "notice-001");
            assert!(notices[0].confirmed);
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_upsert_and_list_external_item_defs() {
        let base_directory = unique_test_path("item-def");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .upsert_external_item_defs(&[
                    ExternalItemDefUpsert {
                        item_id: "30011".to_string(),
                        name_zh: "固源岩".to_string(),
                        item_type: "养成材料".to_string(),
                        rarity: Some(2),
                        raw_json: json!({"item_id": "30011"}),
                    },
                    ExternalItemDefUpsert {
                        item_id: "30104".to_string(),
                        name_zh: "双极纳米片".to_string(),
                        item_type: "养成材料".to_string(),
                        rarity: Some(4),
                        raw_json: json!({"item_id": "30104"}),
                    },
                ])
                .unwrap();

            assert_eq!(repository.count_external_item_defs().unwrap(), 2);

            let items = repository.list_external_item_defs(4).unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].item_id, "30104");
            assert_eq!(items[0].name_zh, "双极纳米片");
            assert_eq!(items[1].item_id, "30011");
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_upsert_and_list_external_operator_defs() {
        let base_directory = unique_test_path("operator-def");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .upsert_external_operator_defs(&[
                    ExternalOperatorDefUpsert {
                        operator_id: "char_009_12fce".to_string(),
                        name_zh: "12F".to_string(),
                        rarity: 1,
                        profession: "术师".to_string(),
                        branch: None,
                        server: "CN".to_string(),
                        raw_json: json!({"operator_id": "char_009_12fce"}),
                    },
                    ExternalOperatorDefUpsert {
                        operator_id: "char_002_amiya".to_string(),
                        name_zh: "阿米娅".to_string(),
                        rarity: 4,
                        profession: "术师".to_string(),
                        branch: Some("中坚术师".to_string()),
                        server: "CN".to_string(),
                        raw_json: json!({"operator_id": "char_002_amiya"}),
                    },
                ])
                .unwrap();

            assert_eq!(repository.count_external_operator_defs().unwrap(), 2);

            let operators = repository.list_external_operator_defs(4).unwrap();
            assert_eq!(operators.len(), 2);
            assert_eq!(operators[0].name_zh, "阿米娅");
            assert_eq!(operators[0].branch.as_deref(), Some("中坚术师"));
            assert_eq!(operators[1].operator_id, "char_009_12fce");
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_replace_external_operator_defs_and_remove_stale_rows() {
        let base_directory = unique_test_path("operator-def-replace");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .upsert_external_operator_defs(&[ExternalOperatorDefUpsert {
                    operator_id: "char_610_acfend".to_string(),
                    name_zh: "Mechanist(卫戍协议)".to_string(),
                    rarity: 5,
                    profession: "重装".to_string(),
                    branch: None,
                    server: "CN".to_string(),
                    raw_json: json!({"operator_id": "char_610_acfend"}),
                }])
                .unwrap();

            repository
                .replace_external_operator_growths(&[ExternalOperatorGrowthUpsert {
                    growth_id: "char_610_acfend:skill_1_2:generic".to_string(),
                    operator_id: "char_610_acfend".to_string(),
                    stage_label: "1→2".to_string(),
                    material_slot: "通用".to_string(),
                    raw_json: json!({
                        "materials": [
                            {
                                "item_name_zh": "技巧概要·卷1",
                                "count": 3
                            }
                        ]
                    }),
                }])
                .unwrap();

            repository
                .replace_external_operator_defs(&[ExternalOperatorDefUpsert {
                    operator_id: "char_002_amiya".to_string(),
                    name_zh: "阿米娅".to_string(),
                    rarity: 4,
                    profession: "术师".to_string(),
                    branch: Some("中坚术师".to_string()),
                    server: "CN".to_string(),
                    raw_json: json!({"operator_id": "char_002_amiya"}),
                }])
                .unwrap();

            assert_eq!(repository.count_external_operator_defs().unwrap(), 1);
            assert_eq!(repository.count_external_operator_growths().unwrap(), 0);

            let operators = repository.list_external_operator_defs(4).unwrap();
            assert_eq!(operators.len(), 1);
            assert_eq!(operators[0].operator_id, "char_002_amiya");
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_replace_and_list_external_operator_growths() {
        let base_directory = unique_test_path("operator-growth");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .replace_external_operator_defs(&[ExternalOperatorDefUpsert {
                    operator_id: "char_103_angel".to_string(),
                    name_zh: "能天使".to_string(),
                    rarity: 5,
                    profession: "狙击".to_string(),
                    branch: Some("速射手".to_string()),
                    server: "CN".to_string(),
                    raw_json: json!({"operator_id": "char_103_angel"}),
                }])
                .unwrap();

            repository
                .replace_external_operator_growths(&[
                    ExternalOperatorGrowthUpsert {
                        growth_id: "char_103_angel:elite_0_1:promotion".to_string(),
                        operator_id: "char_103_angel".to_string(),
                        stage_label: "精英阶段0→1".to_string(),
                        material_slot: "精英化".to_string(),
                        raw_json: json!({
                            "materials": [
                                {"item_name_zh": "龙门币", "count": 30000},
                                {"item_name_zh": "狙击芯片", "count": 5}
                            ]
                        }),
                    },
                    ExternalOperatorGrowthUpsert {
                        growth_id: "char_103_angel:skill_1_2:global".to_string(),
                        operator_id: "char_103_angel".to_string(),
                        stage_label: "1→2".to_string(),
                        material_slot: "通用".to_string(),
                        raw_json: json!({
                            "materials": [
                                {"item_name_zh": "技巧概要·卷1", "count": 5}
                            ]
                        }),
                    },
                ])
                .unwrap();

            assert_eq!(repository.count_external_operator_growths().unwrap(), 2);

            let growth_rows = repository.list_external_operator_growths(8).unwrap();
            assert_eq!(growth_rows.len(), 2);
            assert_eq!(growth_rows[0].operator_name_zh, "能天使");
            assert_eq!(growth_rows[0].stage_label, "1→2");
            assert_eq!(growth_rows[0].material_slot, "通用");
            assert_eq!(growth_rows[0].material_summary, "技巧概要·卷1 x5");
            assert_eq!(growth_rows[1].stage_label, "精英阶段0→1");
            assert!(growth_rows[1].material_summary.contains("龙门币 x30000"));
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_replace_and_list_external_recipes() {
        let base_directory = unique_test_path("recipe-def");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .upsert_external_item_defs(&[
                    ExternalItemDefUpsert {
                        item_id: "30032".to_string(),
                        name_zh: "异铁".to_string(),
                        item_type: "养成材料".to_string(),
                        rarity: Some(1),
                        raw_json: json!({"item_id": "30032"}),
                    },
                    ExternalItemDefUpsert {
                        item_id: "30033".to_string(),
                        name_zh: "异铁组".to_string(),
                        item_type: "养成材料".to_string(),
                        rarity: Some(2),
                        raw_json: json!({"item_id": "30033"}),
                    },
                ])
                .unwrap();

            assert_eq!(
                repository
                    .find_external_item_ids_by_name_zh("异铁组")
                    .unwrap(),
                vec!["30033".to_string()]
            );

            repository
                .replace_external_recipes(&[ExternalRecipeUpsert {
                    recipe_id: "workshop:30033:lv3".to_string(),
                    output_item_id: "30033".to_string(),
                    room_type: "workshop".to_string(),
                    raw_json: json!({
                        "recipe_kind": "精英材料",
                        "workshop_level": 3,
                        "output_item_id": "30033",
                        "output_name_zh": "异铁组",
                        "ingredients": [
                            {
                                "item_id": "30032",
                                "item_name_zh": "异铁",
                                "count": 3,
                            }
                        ],
                        "lmd_cost": 300,
                        "mood_cost": 2,
                    }),
                }])
                .unwrap();

            assert_eq!(repository.count_external_recipes().unwrap(), 1);

            let recipes = repository.list_external_recipes(4).unwrap();
            assert_eq!(recipes.len(), 1);
            assert_eq!(recipes[0].output_name_zh, "异铁组");
            assert_eq!(recipes[0].recipe_kind, "精英材料");
            assert_eq!(recipes[0].workshop_level, 3);
            assert_eq!(recipes[0].ingredient_summary, "异铁 x3");
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_can_upsert_and_list_prts_stage_defs() {
        let base_directory = unique_test_path("stage-def");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());

            repository
                .upsert_external_stage_defs(&[
                    ExternalStageDefUpsert {
                        stage_id: "main_01-07".to_string(),
                        zone_id: None,
                        code: "1-7".to_string(),
                        is_open: true,
                        raw_json: json!({
                            "stage_id": "main_01-07",
                            "code": "1-7",
                            "page_title": "1-7 暴君",
                            "categories": ["主线关卡", "普通难度关卡"],
                        }),
                    },
                    ExternalStageDefUpsert {
                        stage_id: "wk_fly_5".to_string(),
                        zone_id: None,
                        code: "CA-5".to_string(),
                        is_open: true,
                        raw_json: json!({
                            "stage_id": "wk_fly_5",
                            "code": "CA-5",
                            "page_title": "CA-5 战略要道净空",
                            "categories": ["日常关卡"],
                        }),
                    },
                ])
                .unwrap();

            assert_eq!(repository.count_prts_stage_defs().unwrap(), 2);

            let stages = repository.list_prts_stage_defs(8).unwrap();
            assert_eq!(stages.len(), 2);
            assert_eq!(stages[0].code, "1-7");
            assert_eq!(stages[0].page_title.as_deref(), Some("1-7 暴君"));
            assert_eq!(stages[1].code, "CA-5");
            assert_eq!(stages[1].categories, vec!["日常关卡"]);
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn repository_preserves_prts_stage_payload_when_penguin_stage_updates() {
        let base_directory = unique_test_path("stage-merge");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            repository
                .upsert_external_stage_defs(&[ExternalStageDefUpsert {
                    stage_id: "main_01-07".to_string(),
                    zone_id: None,
                    code: "1-7".to_string(),
                    is_open: true,
                    raw_json: json!({
                        "stage_id": "main_01-07",
                        "code": "1-7",
                        "page_title": "1-7 暴君",
                        "categories": ["主线关卡"],
                    }),
                }])
                .unwrap();

            repository
                .replace_penguin_matrix(
                    &[],
                    &[PenguinStageUpsert {
                        stage_id: "main_01-07".to_string(),
                        zone_id: Some("main_1".to_string()),
                        code: "1-7".to_string(),
                        is_open: true,
                        raw_json: json!({
                            "stageId": "main_01-07",
                            "stageType": "MAIN",
                            "code": "1-7",
                        }),
                    }],
                    &[],
                )
                .unwrap();
        }

        let merged = database
            .connection()
            .query_row(
                "SELECT raw_json FROM external_stage_def WHERE stage_id = 'main_01-07'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        let merged = serde_json::from_str::<serde_json::Value>(&merged).unwrap();
        assert_eq!(
            merged.get("stageType").and_then(serde_json::Value::as_str),
            Some("MAIN")
        );
        assert_eq!(
            merged
                .get("prts")
                .and_then(|value| value.get("page_title"))
                .and_then(serde_json::Value::as_str),
            Some("1-7 暴君")
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
