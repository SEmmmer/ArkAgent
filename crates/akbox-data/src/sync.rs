use std::path::Path;

use crate::official::OfficialNoticeClient;
use crate::official::OfficialNoticeClientError;
use crate::penguin::PenguinClient;
use crate::penguin::PenguinClientError;
use crate::prts::PrtsClient;
use crate::prts::PrtsClientError;
use crate::repository::AlertUpsert;
use crate::repository::AppRepository;
use crate::repository::ExternalEventNoticeUpsert;
use crate::repository::ExternalItemDefUpsert;
use crate::repository::ExternalOperatorBuildingSkillUpsert;
use crate::repository::ExternalOperatorDefUpsert;
use crate::repository::ExternalOperatorGrowthUpsert;
use crate::repository::ExternalRecipeUpsert;
use crate::repository::ExternalStageDefUpsert;
use crate::repository::PenguinItemUpsert;
use crate::repository::PenguinMatrixUpsert;
use crate::repository::PenguinStageUpsert;
use crate::repository::RawSourceCacheUpsert;
use thiserror::Error;

pub const OFFICIAL_NOTICE_SOURCE_ID: &str = "official.notice.cn";
pub const OFFICIAL_NOTICE_CACHE_KEY: &str = "official:notice:cn";
pub const PRTS_ITEM_INDEX_SOURCE_ID: &str = "prts.item-index.cn";
pub const PRTS_ITEM_INDEX_CACHE_KEY: &str = "prts:item-index:cn";
pub const PRTS_OPERATOR_INDEX_SOURCE_ID: &str = "prts.operator-index.cn";
pub const PRTS_OPERATOR_INDEX_CACHE_KEY: &str = "prts:operator-index:cn";
pub const PRTS_OPERATOR_GROWTH_SOURCE_ID: &str = "prts.operator-growth.cn";
pub const PRTS_OPERATOR_GROWTH_CACHE_KEY: &str = "prts:operator-growth:cn";
pub const PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID: &str = "prts.operator-building-skill.cn";
pub const PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY: &str = "prts:operator-building-skill:cn";
pub const PRTS_RECIPE_INDEX_SOURCE_ID: &str = "prts.recipe-index.cn";
pub const PRTS_RECIPE_INDEX_CACHE_KEY: &str = "prts:recipe-index:cn";
pub const PRTS_STAGE_INDEX_SOURCE_ID: &str = "prts.stage-index.cn";
pub const PRTS_STAGE_INDEX_CACHE_KEY: &str = "prts:stage-index:cn";
pub const PRTS_SITEINFO_SOURCE_ID: &str = "prts.siteinfo.general";
pub const PRTS_SITEINFO_CACHE_KEY: &str = "prts:siteinfo:general";
pub const PENGUIN_MATRIX_SOURCE_ID: &str = "penguin.matrix.cn";
pub const PENGUIN_MATRIX_CACHE_KEY: &str = "penguin:matrix:cn";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Incremental,
    Full,
}

impl SyncMode {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Incremental => "增量",
            Self::Full => "全量",
        }
    }

    pub fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRunStatus {
    Updated,
    SkippedUnchanged,
}

impl SyncRunStatus {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Updated => "已更新",
            Self::SkippedUnchanged => "未变化，已跳过",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOfficialNoticeOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsSiteInfoOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsOutcome {
    pub site_info: SyncPrtsSiteInfoOutcome,
    pub operator_index: SyncPrtsOperatorIndexOutcome,
    pub item_index: SyncPrtsItemIndexOutcome,
    pub stage_index: SyncPrtsStageIndexOutcome,
    pub recipe_index: SyncPrtsRecipeIndexOutcome,
}

pub type SyncPrtsAllOutcome = SyncPrtsOutcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsItemIndexOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsOperatorIndexOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsOperatorGrowthOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsOperatorBuildingSkillOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsStageIndexOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPrtsRecipeIndexOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPenguinMatrixOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub row_count: usize,
    pub requested_mode: SyncMode,
    pub effective_mode: SyncMode,
    pub run_status: SyncRunStatus,
}

fn can_skip_unchanged_source(
    repository: &AppRepository<'_>,
    source_id: &str,
    cache_key: &str,
    expected_cursor: &str,
    has_local_rows: bool,
) -> Result<Option<usize>, crate::repository::RepositoryError> {
    if !has_local_rows {
        return Ok(None);
    }

    let Some(state) = repository.get_sync_source_state(source_id)? else {
        return Ok(None);
    };
    let Some(cache) = repository.get_raw_source_cache_summary(cache_key)? else {
        return Ok(None);
    };

    if state.cursor_value.as_deref() == Some(expected_cursor) {
        Ok(Some(cache.payload_bytes.max(0) as usize))
    } else {
        Ok(None)
    }
}

fn mark_incremental_skip(
    repository: &AppRepository<'_>,
    source_id: &str,
    cursor_value: &str,
) -> Result<(), crate::repository::RepositoryError> {
    repository.record_sync_success(source_id, Some(cursor_value))?;
    repository.resolve_alert(&sync_failure_alert_id(source_id))
}

fn penguin_cursor_anchor(
    matrix_last_modified: Option<&str>,
    stages_last_modified: Option<&str>,
    items_last_modified: Option<&str>,
) -> Option<String> {
    match (
        matrix_last_modified,
        stages_last_modified,
        items_last_modified,
    ) {
        (Some(matrix), Some(stages), Some(items)) => {
            Some(format!("matrix={matrix}|stages={stages}|items={items}"))
        }
        _ => None,
    }
}

pub fn sync_official_notices(
    repository: &AppRepository<'_>,
    client: &OfficialNoticeClient,
) -> Result<SyncOfficialNoticeOutcome, SyncOfficialNoticeError> {
    sync_official_notices_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_official_notices_with_mode(
    repository: &AppRepository<'_>,
    client: &OfficialNoticeClient,
    requested_mode: SyncMode,
) -> Result<SyncOfficialNoticeOutcome, SyncOfficialNoticeError> {
    repository
        .record_sync_attempt(OFFICIAL_NOTICE_SOURCE_ID)
        .map_err(SyncOfficialNoticeError::Repository)?;

    let response = match client.fetch_notice_index() {
        Ok(response) => response,
        Err(error) => {
            repository
                .record_sync_failure(OFFICIAL_NOTICE_SOURCE_ID, &error.to_string())
                .map_err(SyncOfficialNoticeError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(OFFICIAL_NOTICE_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "官方公告同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncOfficialNoticeError::Repository)?;
            return Err(SyncOfficialNoticeError::Client(error));
        }
    };

    let revision = latest_notice_revision(&response.notices);
    let upserts = response
        .notices
        .iter()
        .map(|notice| ExternalEventNoticeUpsert {
            notice_id: notice.notice_id.clone(),
            title: notice.title.clone(),
            notice_type: notice.notice_type.clone(),
            published_at: notice.published_at.clone(),
            start_at: notice.start_at.clone(),
            end_at: notice.end_at.clone(),
            source_url: notice.source_url.clone(),
            confirmed: true,
            raw_json: notice.raw_json.clone(),
        })
        .collect::<Vec<_>>();

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: OFFICIAL_NOTICE_CACHE_KEY,
            source_name: "official",
            revision: revision.as_deref(),
            content_type: response.content_type.as_str(),
            payload: response.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncOfficialNoticeError::Repository)?;
    repository
        .replace_external_event_notices(&upserts)
        .map_err(SyncOfficialNoticeError::Repository)?;
    repository
        .record_sync_success(OFFICIAL_NOTICE_SOURCE_ID, revision.as_deref())
        .map_err(SyncOfficialNoticeError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(OFFICIAL_NOTICE_SOURCE_ID))
        .map_err(SyncOfficialNoticeError::Repository)?;

    Ok(SyncOfficialNoticeOutcome {
        source_id: OFFICIAL_NOTICE_SOURCE_ID.to_string(),
        cache_key: OFFICIAL_NOTICE_CACHE_KEY.to_string(),
        revision: revision.unwrap_or_else(|| "empty".to_string()),
        cache_size_bytes: response.raw_body.len(),
        row_count: upserts.len(),
        requested_mode,
        effective_mode: SyncMode::Full,
        run_status: SyncRunStatus::Updated,
    })
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
        requested_mode: SyncMode::Full,
        effective_mode: SyncMode::Full,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts_item_index(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
) -> Result<SyncPrtsItemIndexOutcome, SyncPrtsItemIndexError> {
    sync_prts_item_index_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_prts_item_index_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    requested_mode: SyncMode,
) -> Result<SyncPrtsItemIndexOutcome, SyncPrtsItemIndexError> {
    repository
        .record_sync_attempt(PRTS_ITEM_INDEX_SOURCE_ID)
        .map_err(SyncPrtsItemIndexError::Repository)?;

    if !requested_mode.is_full() {
        let revision = client
            .fetch_item_index_revision()
            .map_err(SyncPrtsItemIndexError::Client)?;
        let existing_row_count = repository
            .count_external_item_defs()
            .map_err(SyncPrtsItemIndexError::Repository)?;
        if let Some(cache_size_bytes) = can_skip_unchanged_source(
            repository,
            PRTS_ITEM_INDEX_SOURCE_ID,
            PRTS_ITEM_INDEX_CACHE_KEY,
            revision.as_str(),
            existing_row_count > 0,
        )
        .map_err(SyncPrtsItemIndexError::Repository)?
        {
            mark_incremental_skip(repository, PRTS_ITEM_INDEX_SOURCE_ID, revision.as_str())
                .map_err(SyncPrtsItemIndexError::Repository)?;
            return Ok(SyncPrtsItemIndexOutcome {
                source_id: PRTS_ITEM_INDEX_SOURCE_ID.to_string(),
                cache_key: PRTS_ITEM_INDEX_CACHE_KEY.to_string(),
                revision,
                cache_size_bytes,
                row_count: existing_row_count as usize,
                requested_mode,
                effective_mode: SyncMode::Incremental,
                run_status: SyncRunStatus::SkippedUnchanged,
            });
        }
    }

    let item_index = match client.fetch_item_index() {
        Ok(item_index) => item_index,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_ITEM_INDEX_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsItemIndexError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_ITEM_INDEX_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 道具索引同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsItemIndexError::Repository)?;
            return Err(SyncPrtsItemIndexError::Client(error));
        }
    };

    let upserts = item_index
        .items
        .iter()
        .map(|item| ExternalItemDefUpsert {
            item_id: item.item_id.clone(),
            name_zh: item.name_zh.clone(),
            item_type: item.item_type.clone(),
            rarity: item.rarity,
            raw_json: item.raw_json.clone(),
        })
        .collect::<Vec<_>>();

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_ITEM_INDEX_CACHE_KEY,
            source_name: "prts",
            revision: Some(item_index.revision.as_str()),
            content_type: item_index.content_type.as_str(),
            payload: item_index.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsItemIndexError::Repository)?;
    repository
        .upsert_external_item_defs(&upserts)
        .map_err(SyncPrtsItemIndexError::Repository)?;
    let stored_row_count = repository
        .count_external_item_defs()
        .map_err(SyncPrtsItemIndexError::Repository)? as usize;
    repository
        .record_sync_success(
            PRTS_ITEM_INDEX_SOURCE_ID,
            Some(item_index.revision.as_str()),
        )
        .map_err(SyncPrtsItemIndexError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PRTS_ITEM_INDEX_SOURCE_ID))
        .map_err(SyncPrtsItemIndexError::Repository)?;

    Ok(SyncPrtsItemIndexOutcome {
        source_id: PRTS_ITEM_INDEX_SOURCE_ID.to_string(),
        cache_key: PRTS_ITEM_INDEX_CACHE_KEY.to_string(),
        revision: item_index.revision,
        cache_size_bytes: item_index.raw_body.len(),
        row_count: stored_row_count,
        requested_mode,
        effective_mode: requested_mode,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts_operator_index(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
) -> Result<SyncPrtsOperatorIndexOutcome, SyncPrtsOperatorIndexError> {
    sync_prts_operator_index_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_prts_operator_index_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    requested_mode: SyncMode,
) -> Result<SyncPrtsOperatorIndexOutcome, SyncPrtsOperatorIndexError> {
    repository
        .record_sync_attempt(PRTS_OPERATOR_INDEX_SOURCE_ID)
        .map_err(SyncPrtsOperatorIndexError::Repository)?;

    if !requested_mode.is_full() {
        let revision = client
            .fetch_operator_index_revision()
            .map_err(SyncPrtsOperatorIndexError::Client)?;
        let existing_row_count = repository
            .count_external_operator_defs()
            .map_err(SyncPrtsOperatorIndexError::Repository)?;
        if let Some(cache_size_bytes) = can_skip_unchanged_source(
            repository,
            PRTS_OPERATOR_INDEX_SOURCE_ID,
            PRTS_OPERATOR_INDEX_CACHE_KEY,
            revision.as_str(),
            existing_row_count > 0,
        )
        .map_err(SyncPrtsOperatorIndexError::Repository)?
        {
            mark_incremental_skip(repository, PRTS_OPERATOR_INDEX_SOURCE_ID, revision.as_str())
                .map_err(SyncPrtsOperatorIndexError::Repository)?;
            return Ok(SyncPrtsOperatorIndexOutcome {
                source_id: PRTS_OPERATOR_INDEX_SOURCE_ID.to_string(),
                cache_key: PRTS_OPERATOR_INDEX_CACHE_KEY.to_string(),
                revision,
                cache_size_bytes,
                row_count: existing_row_count as usize,
                requested_mode,
                effective_mode: SyncMode::Incremental,
                run_status: SyncRunStatus::SkippedUnchanged,
            });
        }
    }

    let operator_index = match client.fetch_operator_index() {
        Ok(operator_index) => operator_index,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_OPERATOR_INDEX_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsOperatorIndexError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_OPERATOR_INDEX_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 干员索引同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsOperatorIndexError::Repository)?;
            return Err(SyncPrtsOperatorIndexError::Client(error));
        }
    };

    let upserts = operator_index
        .operators
        .iter()
        .filter(|operator| operator.is_box_collectible)
        .map(|operator| ExternalOperatorDefUpsert {
            operator_id: operator.operator_id.clone(),
            name_zh: operator.name_zh.clone(),
            rarity: operator.rarity,
            profession: operator.profession.clone(),
            branch: operator.branch.clone(),
            server: "CN".to_string(),
            raw_json: operator.raw_json.clone(),
        })
        .collect::<Vec<_>>();

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_OPERATOR_INDEX_CACHE_KEY,
            source_name: "prts",
            revision: Some(operator_index.revision.as_str()),
            content_type: operator_index.content_type.as_str(),
            payload: operator_index.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsOperatorIndexError::Repository)?;
    repository
        .replace_external_operator_defs(&upserts)
        .map_err(SyncPrtsOperatorIndexError::Repository)?;
    repository
        .record_sync_success(
            PRTS_OPERATOR_INDEX_SOURCE_ID,
            Some(operator_index.revision.as_str()),
        )
        .map_err(SyncPrtsOperatorIndexError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PRTS_OPERATOR_INDEX_SOURCE_ID))
        .map_err(SyncPrtsOperatorIndexError::Repository)?;

    Ok(SyncPrtsOperatorIndexOutcome {
        source_id: PRTS_OPERATOR_INDEX_SOURCE_ID.to_string(),
        cache_key: PRTS_OPERATOR_INDEX_CACHE_KEY.to_string(),
        revision: operator_index.revision,
        cache_size_bytes: operator_index.raw_body.len(),
        row_count: upserts.len(),
        requested_mode,
        effective_mode: requested_mode,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts_operator_growth(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
) -> Result<SyncPrtsOperatorGrowthOutcome, SyncPrtsOperatorGrowthError> {
    sync_prts_operator_growth_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_prts_operator_growth_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    requested_mode: SyncMode,
) -> Result<SyncPrtsOperatorGrowthOutcome, SyncPrtsOperatorGrowthError> {
    repository
        .record_sync_attempt(PRTS_OPERATOR_GROWTH_SOURCE_ID)
        .map_err(SyncPrtsOperatorGrowthError::Repository)?;

    let operator_growth = match client.fetch_operator_growth() {
        Ok(operator_growth) => operator_growth,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_OPERATOR_GROWTH_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsOperatorGrowthError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_OPERATOR_GROWTH_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 养成需求同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsOperatorGrowthError::Repository)?;
            return Err(SyncPrtsOperatorGrowthError::Client(error));
        }
    };

    let operator_upserts = operator_growth
        .operators
        .iter()
        .map(|operator| ExternalOperatorDefUpsert {
            operator_id: operator.operator_id.clone(),
            name_zh: operator.name_zh.clone(),
            rarity: operator.rarity,
            profession: operator.profession.clone(),
            branch: operator.branch.clone(),
            server: "CN".to_string(),
            raw_json: operator.raw_json.clone(),
        })
        .collect::<Vec<_>>();
    let upserts = build_prts_operator_growth_upserts(&operator_growth.growths);

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_OPERATOR_GROWTH_CACHE_KEY,
            source_name: "prts",
            revision: Some(operator_growth.revision.as_str()),
            content_type: operator_growth.content_type.as_str(),
            payload: operator_growth.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsOperatorGrowthError::Repository)?;
    repository
        .upsert_external_operator_defs(&operator_upserts)
        .map_err(SyncPrtsOperatorGrowthError::Repository)?;
    repository
        .replace_external_operator_growths(&upserts)
        .map_err(SyncPrtsOperatorGrowthError::Repository)?;
    repository
        .record_sync_success(
            PRTS_OPERATOR_GROWTH_SOURCE_ID,
            Some(operator_growth.revision.as_str()),
        )
        .map_err(SyncPrtsOperatorGrowthError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PRTS_OPERATOR_GROWTH_SOURCE_ID))
        .map_err(SyncPrtsOperatorGrowthError::Repository)?;

    Ok(SyncPrtsOperatorGrowthOutcome {
        source_id: PRTS_OPERATOR_GROWTH_SOURCE_ID.to_string(),
        cache_key: PRTS_OPERATOR_GROWTH_CACHE_KEY.to_string(),
        revision: operator_growth.revision,
        cache_size_bytes: operator_growth.raw_body.len(),
        row_count: upserts.len(),
        requested_mode,
        effective_mode: SyncMode::Full,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts_operator_building_skill(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
) -> Result<SyncPrtsOperatorBuildingSkillOutcome, SyncPrtsOperatorBuildingSkillError> {
    sync_prts_operator_building_skill_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_prts_operator_building_skill_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    requested_mode: SyncMode,
) -> Result<SyncPrtsOperatorBuildingSkillOutcome, SyncPrtsOperatorBuildingSkillError> {
    repository
        .record_sync_attempt(PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID)
        .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;

    let building_skill_index = match client.fetch_operator_building_skills() {
        Ok(building_skill_index) => building_skill_index,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 基建技能同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;
            return Err(SyncPrtsOperatorBuildingSkillError::Client(error));
        }
    };

    let operator_upserts = building_skill_index
        .operators
        .iter()
        .map(|operator| ExternalOperatorDefUpsert {
            operator_id: operator.operator_id.clone(),
            name_zh: operator.name_zh.clone(),
            rarity: operator.rarity,
            profession: operator.profession.clone(),
            branch: operator.branch.clone(),
            server: "CN".to_string(),
            raw_json: operator.raw_json.clone(),
        })
        .collect::<Vec<_>>();
    let upserts = build_prts_operator_building_skill_upserts(&building_skill_index.building_skills);

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY,
            source_name: "prts",
            revision: Some(building_skill_index.revision.as_str()),
            content_type: building_skill_index.content_type.as_str(),
            payload: building_skill_index.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;
    repository
        .upsert_external_operator_defs(&operator_upserts)
        .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;
    repository
        .replace_external_operator_building_skills(&upserts)
        .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;
    repository
        .record_sync_success(
            PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID,
            Some(building_skill_index.revision.as_str()),
        )
        .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(
            PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID,
        ))
        .map_err(SyncPrtsOperatorBuildingSkillError::Repository)?;

    Ok(SyncPrtsOperatorBuildingSkillOutcome {
        source_id: PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID.to_string(),
        cache_key: PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY.to_string(),
        revision: building_skill_index.revision,
        cache_size_bytes: building_skill_index.raw_body.len(),
        row_count: upserts.len(),
        requested_mode,
        effective_mode: SyncMode::Full,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts_stage_index(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
) -> Result<SyncPrtsStageIndexOutcome, SyncPrtsStageIndexError> {
    sync_prts_stage_index_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_prts_stage_index_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    requested_mode: SyncMode,
) -> Result<SyncPrtsStageIndexOutcome, SyncPrtsStageIndexError> {
    repository
        .record_sync_attempt(PRTS_STAGE_INDEX_SOURCE_ID)
        .map_err(SyncPrtsStageIndexError::Repository)?;

    if !requested_mode.is_full() {
        let revision = client
            .fetch_stage_index_revision()
            .map_err(SyncPrtsStageIndexError::Client)?;
        let existing_row_count = repository
            .count_prts_stage_defs()
            .map_err(SyncPrtsStageIndexError::Repository)?;
        if let Some(cache_size_bytes) = can_skip_unchanged_source(
            repository,
            PRTS_STAGE_INDEX_SOURCE_ID,
            PRTS_STAGE_INDEX_CACHE_KEY,
            revision.as_str(),
            existing_row_count > 0,
        )
        .map_err(SyncPrtsStageIndexError::Repository)?
        {
            mark_incremental_skip(repository, PRTS_STAGE_INDEX_SOURCE_ID, revision.as_str())
                .map_err(SyncPrtsStageIndexError::Repository)?;
            return Ok(SyncPrtsStageIndexOutcome {
                source_id: PRTS_STAGE_INDEX_SOURCE_ID.to_string(),
                cache_key: PRTS_STAGE_INDEX_CACHE_KEY.to_string(),
                revision,
                cache_size_bytes,
                row_count: existing_row_count as usize,
                requested_mode,
                effective_mode: SyncMode::Incremental,
                run_status: SyncRunStatus::SkippedUnchanged,
            });
        }
    }

    let stage_index = match client.fetch_stage_index() {
        Ok(stage_index) => stage_index,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_STAGE_INDEX_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsStageIndexError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_STAGE_INDEX_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 关卡索引同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsStageIndexError::Repository)?;
            return Err(SyncPrtsStageIndexError::Client(error));
        }
    };

    let upserts = stage_index
        .stages
        .iter()
        .map(|stage| ExternalStageDefUpsert {
            stage_id: stage.stage_id.clone(),
            zone_id: stage.zone_id.clone(),
            code: stage.code.clone(),
            is_open: true,
            raw_json: stage.raw_json.clone(),
        })
        .collect::<Vec<_>>();

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_STAGE_INDEX_CACHE_KEY,
            source_name: "prts",
            revision: Some(stage_index.revision.as_str()),
            content_type: stage_index.content_type.as_str(),
            payload: stage_index.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsStageIndexError::Repository)?;
    repository
        .upsert_external_stage_defs(&upserts)
        .map_err(SyncPrtsStageIndexError::Repository)?;
    repository
        .record_sync_success(
            PRTS_STAGE_INDEX_SOURCE_ID,
            Some(stage_index.revision.as_str()),
        )
        .map_err(SyncPrtsStageIndexError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PRTS_STAGE_INDEX_SOURCE_ID))
        .map_err(SyncPrtsStageIndexError::Repository)?;

    Ok(SyncPrtsStageIndexOutcome {
        source_id: PRTS_STAGE_INDEX_SOURCE_ID.to_string(),
        cache_key: PRTS_STAGE_INDEX_CACHE_KEY.to_string(),
        revision: stage_index.revision,
        cache_size_bytes: stage_index.raw_body.len(),
        row_count: upserts.len(),
        requested_mode,
        effective_mode: requested_mode,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts_recipe_index(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
) -> Result<SyncPrtsRecipeIndexOutcome, SyncPrtsRecipeIndexError> {
    sync_prts_recipe_index_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_prts_recipe_index_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    requested_mode: SyncMode,
) -> Result<SyncPrtsRecipeIndexOutcome, SyncPrtsRecipeIndexError> {
    repository
        .record_sync_attempt(PRTS_RECIPE_INDEX_SOURCE_ID)
        .map_err(SyncPrtsRecipeIndexError::Repository)?;

    if !requested_mode.is_full() {
        let revision = client
            .fetch_recipe_index_revision()
            .map_err(SyncPrtsRecipeIndexError::Client)?;
        let existing_row_count = repository
            .count_external_recipes()
            .map_err(SyncPrtsRecipeIndexError::Repository)?;
        if let Some(cache_size_bytes) = can_skip_unchanged_source(
            repository,
            PRTS_RECIPE_INDEX_SOURCE_ID,
            PRTS_RECIPE_INDEX_CACHE_KEY,
            revision.as_str(),
            existing_row_count > 0,
        )
        .map_err(SyncPrtsRecipeIndexError::Repository)?
        {
            mark_incremental_skip(repository, PRTS_RECIPE_INDEX_SOURCE_ID, revision.as_str())
                .map_err(SyncPrtsRecipeIndexError::Repository)?;
            return Ok(SyncPrtsRecipeIndexOutcome {
                source_id: PRTS_RECIPE_INDEX_SOURCE_ID.to_string(),
                cache_key: PRTS_RECIPE_INDEX_CACHE_KEY.to_string(),
                revision,
                cache_size_bytes,
                row_count: existing_row_count as usize,
                requested_mode,
                effective_mode: SyncMode::Incremental,
                run_status: SyncRunStatus::SkippedUnchanged,
            });
        }
    }

    let recipe_index = match client.fetch_recipe_index() {
        Ok(recipe_index) => recipe_index,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_RECIPE_INDEX_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsRecipeIndexError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_RECIPE_INDEX_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 配方同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsRecipeIndexError::Repository)?;
            return Err(SyncPrtsRecipeIndexError::Client(error));
        }
    };

    repository
        .upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: PRTS_RECIPE_INDEX_CACHE_KEY,
            source_name: "prts",
            revision: Some(recipe_index.revision.as_str()),
            content_type: recipe_index.content_type.as_str(),
            payload: recipe_index.raw_body.as_slice(),
            expires_at: None,
        })
        .map_err(SyncPrtsRecipeIndexError::Repository)?;

    let upserts = match build_prts_recipe_upserts(repository, &recipe_index.recipes) {
        Ok(upserts) => upserts,
        Err(error) => {
            repository
                .record_sync_failure(PRTS_RECIPE_INDEX_SOURCE_ID, &error.to_string())
                .map_err(SyncPrtsRecipeIndexError::Repository)?;
            repository
                .upsert_alert(&AlertUpsert {
                    alert_id: &sync_failure_alert_id(PRTS_RECIPE_INDEX_SOURCE_ID),
                    alert_type: "sync_failure",
                    severity: "error",
                    title: "PRTS 配方同步失败",
                    message: &error.to_string(),
                    status: "active",
                    payload_json: None,
                })
                .map_err(SyncPrtsRecipeIndexError::Repository)?;
            return Err(error);
        }
    };

    repository
        .replace_external_recipes(&upserts)
        .map_err(SyncPrtsRecipeIndexError::Repository)?;
    repository
        .record_sync_success(
            PRTS_RECIPE_INDEX_SOURCE_ID,
            Some(recipe_index.revision.as_str()),
        )
        .map_err(SyncPrtsRecipeIndexError::Repository)?;
    repository
        .resolve_alert(&sync_failure_alert_id(PRTS_RECIPE_INDEX_SOURCE_ID))
        .map_err(SyncPrtsRecipeIndexError::Repository)?;

    Ok(SyncPrtsRecipeIndexOutcome {
        source_id: PRTS_RECIPE_INDEX_SOURCE_ID.to_string(),
        cache_key: PRTS_RECIPE_INDEX_CACHE_KEY.to_string(),
        revision: recipe_index.revision,
        cache_size_bytes: recipe_index.raw_body.len(),
        row_count: upserts.len(),
        requested_mode,
        effective_mode: requested_mode,
        run_status: SyncRunStatus::Updated,
    })
}

pub fn sync_prts(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    base_directory: &Path,
) -> Result<SyncPrtsOutcome, SyncPrtsSyncError> {
    sync_prts_with_mode(repository, client, base_directory, SyncMode::Full)
}

pub fn sync_prts_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    base_directory: &Path,
    requested_mode: SyncMode,
) -> Result<SyncPrtsOutcome, SyncPrtsSyncError> {
    let site_info = sync_prts_site_info(repository, client, base_directory)
        .map_err(SyncPrtsSyncError::SiteInfo)?;
    let operator_index = sync_prts_operator_index_with_mode(repository, client, requested_mode)
        .map_err(SyncPrtsSyncError::OperatorIndex)?;
    let item_index = sync_prts_item_index_with_mode(repository, client, requested_mode)
        .map_err(SyncPrtsSyncError::ItemIndex)?;
    let stage_index = sync_prts_stage_index_with_mode(repository, client, requested_mode)
        .map_err(SyncPrtsSyncError::StageIndex)?;
    let recipe_index = sync_prts_recipe_index_with_mode(repository, client, requested_mode)
        .map_err(SyncPrtsSyncError::RecipeIndex)?;

    Ok(SyncPrtsOutcome {
        site_info,
        operator_index,
        item_index,
        stage_index,
        recipe_index,
    })
}

pub fn sync_prts_all(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    base_directory: &Path,
) -> Result<SyncPrtsOutcome, SyncPrtsSyncError> {
    sync_prts(repository, client, base_directory)
}

pub fn sync_prts_all_with_mode(
    repository: &AppRepository<'_>,
    client: &PrtsClient,
    base_directory: &Path,
    requested_mode: SyncMode,
) -> Result<SyncPrtsOutcome, SyncPrtsSyncError> {
    sync_prts_with_mode(repository, client, base_directory, requested_mode)
}

pub fn sync_penguin_matrix(
    repository: &AppRepository<'_>,
    client: &PenguinClient,
) -> Result<SyncPenguinMatrixOutcome, SyncPenguinError> {
    sync_penguin_matrix_with_mode(repository, client, SyncMode::Full)
}

pub fn sync_penguin_matrix_with_mode(
    repository: &AppRepository<'_>,
    client: &PenguinClient,
    requested_mode: SyncMode,
) -> Result<SyncPenguinMatrixOutcome, SyncPenguinError> {
    let fail_sync =
        |error: PenguinClientError| -> Result<SyncPenguinMatrixOutcome, SyncPenguinError> {
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

            Err(SyncPenguinError::Client(error))
        };

    repository
        .record_sync_attempt(PENGUIN_MATRIX_SOURCE_ID)
        .map_err(SyncPenguinError::Repository)?;

    if !requested_mode.is_full() {
        let matrix_last_modified = client
            .fetch_cn_matrix_last_modified()
            .map_err(SyncPenguinError::Client)?;
        let stages_last_modified = client
            .fetch_cn_stages_last_modified()
            .map_err(SyncPenguinError::Client)?;
        let items_last_modified = client
            .fetch_cn_items_last_modified()
            .map_err(SyncPenguinError::Client)?;
        if let Some(cursor_anchor) = penguin_cursor_anchor(
            matrix_last_modified.as_deref(),
            stages_last_modified.as_deref(),
            items_last_modified.as_deref(),
        ) {
            let existing_row_count = repository
                .count_external_drop_matrix()
                .map_err(SyncPenguinError::Repository)?;
            if let Some(cache_size_bytes) = can_skip_unchanged_source(
                repository,
                PENGUIN_MATRIX_SOURCE_ID,
                PENGUIN_MATRIX_CACHE_KEY,
                cursor_anchor.as_str(),
                existing_row_count > 0,
            )
            .map_err(SyncPenguinError::Repository)?
            {
                let revision = repository
                    .get_raw_source_cache_summary(PENGUIN_MATRIX_CACHE_KEY)
                    .map_err(SyncPenguinError::Repository)?
                    .and_then(|summary| summary.revision)
                    .unwrap_or_else(|| "unknown".to_string());
                mark_incremental_skip(repository, PENGUIN_MATRIX_SOURCE_ID, cursor_anchor.as_str())
                    .map_err(SyncPenguinError::Repository)?;
                return Ok(SyncPenguinMatrixOutcome {
                    source_id: PENGUIN_MATRIX_SOURCE_ID.to_string(),
                    cache_key: PENGUIN_MATRIX_CACHE_KEY.to_string(),
                    revision,
                    cache_size_bytes,
                    row_count: existing_row_count as usize,
                    requested_mode,
                    effective_mode: SyncMode::Incremental,
                    run_status: SyncRunStatus::SkippedUnchanged,
                });
            }
        }
    }

    let (matrix, stages, items) = match (
        client.fetch_cn_matrix(),
        client.fetch_cn_stages(),
        client.fetch_cn_items(),
    ) {
        (Ok(matrix), Ok(stages), Ok(items)) => (matrix, stages, items),
        (Err(error), _, _) => return fail_sync(error),
        (_, Err(error), _) => return fail_sync(error),
        (_, _, Err(error)) => return fail_sync(error),
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
    let stage_upserts = stages
        .stages
        .iter()
        .map(|stage| PenguinStageUpsert {
            stage_id: stage.stage_id.clone(),
            zone_id: stage.zone_id.clone(),
            code: stage.code.clone(),
            is_open: stage
                .existence
                .get("CN")
                .and_then(|value| value.get("exist"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            raw_json: serde_json::to_value(stage).expect("penguin stage should be serializable"),
        })
        .collect::<Vec<_>>();
    let item_upserts = items
        .items
        .iter()
        .map(|item| PenguinItemUpsert {
            item_id: item.item_id.clone(),
            name_zh: item.name.clone(),
            item_type: item.item_type.clone(),
            rarity: item.rarity,
            raw_json: serde_json::to_value(item).expect("penguin item should be serializable"),
        })
        .collect::<Vec<_>>();

    repository
        .replace_penguin_matrix(&upserts, &stage_upserts, &item_upserts)
        .map_err(SyncPenguinError::Repository)?;
    let success_cursor = if requested_mode.is_full() {
        revision.clone()
    } else {
        let matrix_last_modified = client
            .fetch_cn_matrix_last_modified()
            .map_err(SyncPenguinError::Client)?;
        let stages_last_modified = client
            .fetch_cn_stages_last_modified()
            .map_err(SyncPenguinError::Client)?;
        let items_last_modified = client
            .fetch_cn_items_last_modified()
            .map_err(SyncPenguinError::Client)?;
        penguin_cursor_anchor(
            matrix_last_modified.as_deref(),
            stages_last_modified.as_deref(),
            items_last_modified.as_deref(),
        )
        .unwrap_or_else(|| revision.clone())
    };
    repository
        .record_sync_success(PENGUIN_MATRIX_SOURCE_ID, Some(success_cursor.as_str()))
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
        requested_mode,
        effective_mode: requested_mode,
        run_status: SyncRunStatus::Updated,
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
pub enum SyncPrtsItemIndexError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

#[derive(Debug, Error)]
pub enum SyncPrtsOperatorIndexError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

#[derive(Debug, Error)]
pub enum SyncPrtsOperatorGrowthError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

#[derive(Debug, Error)]
pub enum SyncPrtsOperatorBuildingSkillError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

#[derive(Debug, Error)]
pub enum SyncPrtsStageIndexError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
}

#[derive(Debug, Error)]
pub enum SyncPrtsRecipeIndexError {
    #[error(transparent)]
    Client(#[from] PrtsClientError),
    #[error(transparent)]
    Repository(#[from] crate::repository::RepositoryError),
    #[error("failed to resolve PRTS recipe item names into item_id: {message}")]
    ResolveRecipeItemIds { message: String },
}

#[derive(Debug, Error)]
pub enum SyncPrtsSyncError {
    #[error("PRTS 站点信息同步失败：{0}")]
    SiteInfo(#[source] SyncPrtsError),
    #[error("PRTS 干员索引同步失败：{0}")]
    OperatorIndex(#[source] SyncPrtsOperatorIndexError),
    #[error("PRTS 道具索引同步失败：{0}")]
    ItemIndex(#[source] SyncPrtsItemIndexError),
    #[error("PRTS 关卡索引同步失败：{0}")]
    StageIndex(#[source] SyncPrtsStageIndexError),
    #[error("PRTS 配方同步失败：{0}")]
    RecipeIndex(#[source] SyncPrtsRecipeIndexError),
}

pub type SyncPrtsAllError = SyncPrtsSyncError;

#[derive(Debug, Error)]
pub enum SyncOfficialNoticeError {
    #[error(transparent)]
    Client(#[from] OfficialNoticeClientError),
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

fn build_prts_operator_growth_upserts(
    growths: &[crate::prts::PrtsOperatorGrowthDefinition],
) -> Vec<ExternalOperatorGrowthUpsert> {
    growths
        .iter()
        .map(|growth| {
            let stage_key = growth
                .raw_json
                .get("stage_key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown-stage");
            let material_slot_key = growth
                .raw_json
                .get("material_slot_key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown-slot");

            ExternalOperatorGrowthUpsert {
                growth_id: format!("{}:{stage_key}:{material_slot_key}", growth.operator_id),
                operator_id: growth.operator_id.clone(),
                stage_label: growth.stage_label.clone(),
                material_slot: growth.material_slot.clone(),
                raw_json: growth.raw_json.clone(),
            }
        })
        .collect()
}

fn build_prts_operator_building_skill_upserts(
    building_skills: &[crate::prts::PrtsOperatorBuildingSkillDefinition],
) -> Vec<ExternalOperatorBuildingSkillUpsert> {
    building_skills
        .iter()
        .enumerate()
        .map(|(index, building_skill)| {
            let condition_key = building_skill
                .raw_json
                .get("condition_key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown-condition");
            let room_type_key = building_skill
                .raw_json
                .get("room_type_key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(building_skill.room_type.as_str());

            ExternalOperatorBuildingSkillUpsert {
                skill_id: format!(
                    "{}:{condition_key}:{room_type_key}:row{}",
                    building_skill.operator_id,
                    index + 1
                ),
                operator_id: building_skill.operator_id.clone(),
                room_type: building_skill.room_type.clone(),
                skill_name: building_skill.skill_name.clone(),
                raw_json: building_skill.raw_json.clone(),
            }
        })
        .collect()
}

fn build_prts_recipe_upserts(
    repository: &AppRepository<'_>,
    recipes: &[crate::prts::PrtsRecipeDefinition],
) -> Result<Vec<ExternalRecipeUpsert>, SyncPrtsRecipeIndexError> {
    let mut upserts = Vec::with_capacity(recipes.len());
    let mut resolution_failures = Vec::new();

    for (recipe_index, recipe) in recipes.iter().enumerate() {
        let output_item_id =
            match resolve_external_item_id_by_name(repository, &recipe.output_name_zh)? {
                Some(item_id) => item_id,
                None => {
                    resolution_failures
                        .push(format!("产物 `{}` 未匹配到 item_id", recipe.output_name_zh));
                    continue;
                }
            };

        let mut ingredient_values = Vec::with_capacity(recipe.ingredients.len());
        let mut can_write_recipe = true;
        for ingredient in &recipe.ingredients {
            match resolve_external_item_id_by_name(repository, &ingredient.item_name_zh)? {
                Some(item_id) => ingredient_values.push(serde_json::json!({
                    "item_id": item_id,
                    "item_name_zh": ingredient.item_name_zh,
                    "count": ingredient.count,
                })),
                None => {
                    resolution_failures.push(format!(
                        "原料 `{}` 未匹配到 item_id",
                        ingredient.item_name_zh
                    ));
                    can_write_recipe = false;
                }
            }
        }

        if !can_write_recipe {
            continue;
        }

        upserts.push(ExternalRecipeUpsert {
            recipe_id: format!(
                "workshop:{output_item_id}:lv{}:row{}",
                recipe.workshop_level,
                recipe_index + 1
            ),
            output_item_id: output_item_id.clone(),
            room_type: "workshop".to_string(),
            raw_json: serde_json::json!({
                "recipe_kind": recipe.recipe_kind.clone(),
                "workshop_level": recipe.workshop_level,
                "room_type": "workshop",
                "output_item_id": output_item_id,
                "output_name_zh": recipe.output_name_zh.clone(),
                "ingredients": ingredient_values,
                "lmd_cost": recipe.lmd_cost,
                "mood_cost": recipe.mood_cost,
                "byproduct_rate": recipe.byproduct_rate,
                "unlock_condition": recipe.unlock_condition.clone(),
            }),
        });
    }

    if resolution_failures.is_empty() {
        Ok(upserts)
    } else {
        resolution_failures.sort();
        resolution_failures.dedup();
        Err(SyncPrtsRecipeIndexError::ResolveRecipeItemIds {
            message: resolution_failures.join("；"),
        })
    }
}

fn resolve_external_item_id_by_name(
    repository: &AppRepository<'_>,
    name_zh: &str,
) -> Result<Option<String>, SyncPrtsRecipeIndexError> {
    let item_matches = repository
        .find_external_item_matches_by_name_zh(name_zh)
        .map_err(SyncPrtsRecipeIndexError::Repository)?;

    match item_matches.as_slice() {
        [] => Ok(None),
        [item_match] => Ok(Some(item_match.item_id.clone())),
        _ => resolve_preferred_item_match(name_zh, &item_matches),
    }
}

fn resolve_preferred_item_match(
    name_zh: &str,
    item_matches: &[crate::repository::ExternalItemNameMatchRecord],
) -> Result<Option<String>, SyncPrtsRecipeIndexError> {
    let mut ranked_matches = item_matches
        .iter()
        .map(|item_match| (item_match_resolution_score(item_match), item_match))
        .collect::<Vec<_>>();
    ranked_matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.item_id.cmp(&right.1.item_id))
    });

    let Some((best_score, best_match)) = ranked_matches.first() else {
        return Ok(None);
    };

    let second_score = ranked_matches.get(1).map(|value| value.0);
    if second_score.is_none_or(|score| *best_score > score) {
        return Ok(Some(best_match.item_id.clone()));
    }

    let item_ids = item_matches
        .iter()
        .map(|item_match| item_match.item_id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(SyncPrtsRecipeIndexError::ResolveRecipeItemIds {
        message: format!("道具 `{name_zh}` 匹配到多个 item_id: {item_ids}"),
    })
}

fn item_match_resolution_score(item_match: &crate::repository::ExternalItemNameMatchRecord) -> i32 {
    let mut score = 0_i32;
    if item_match.has_prts_payload {
        score += 1_000;
    }

    match item_match.penguin_sort_id.as_deref() {
        Some(sort_id) if sort_id == item_match.item_id => score += 100,
        Some(_) => score -= 100,
        None => {}
    }

    score
}

fn matrix_revision(rows: &[crate::penguin::PenguinMatrixRow]) -> String {
    rows.iter()
        .map(|row| row.end.unwrap_or(row.start))
        .max()
        .unwrap_or(0)
        .to_string()
}

fn latest_notice_revision(notices: &[crate::official::OfficialNoticeFeedEntry]) -> Option<String> {
    notices
        .iter()
        .map(|notice| notice.published_at.clone())
        .max()
}

#[cfg(test)]
mod tests {
    use super::OFFICIAL_NOTICE_CACHE_KEY;
    use super::OFFICIAL_NOTICE_SOURCE_ID;
    use super::PENGUIN_MATRIX_CACHE_KEY;
    use super::PENGUIN_MATRIX_SOURCE_ID;
    use super::PRTS_ITEM_INDEX_CACHE_KEY;
    use super::PRTS_ITEM_INDEX_SOURCE_ID;
    use super::PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY;
    use super::PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID;
    use super::PRTS_OPERATOR_GROWTH_CACHE_KEY;
    use super::PRTS_OPERATOR_GROWTH_SOURCE_ID;
    use super::PRTS_OPERATOR_INDEX_CACHE_KEY;
    use super::PRTS_OPERATOR_INDEX_SOURCE_ID;
    use super::PRTS_RECIPE_INDEX_CACHE_KEY;
    use super::PRTS_RECIPE_INDEX_SOURCE_ID;
    use super::PRTS_SITEINFO_CACHE_KEY;
    use super::PRTS_SITEINFO_SOURCE_ID;
    use super::PRTS_STAGE_INDEX_CACHE_KEY;
    use super::PRTS_STAGE_INDEX_SOURCE_ID;
    use super::SyncMode;
    use super::SyncRunStatus;
    use super::resolve_external_item_id_by_name;
    use super::sync_official_notices;
    use super::sync_penguin_matrix;
    use super::sync_penguin_matrix_with_mode;
    use super::sync_prts;
    use super::sync_prts_item_index;
    use super::sync_prts_item_index_with_mode;
    use super::sync_prts_operator_building_skill;
    use super::sync_prts_operator_growth;
    use super::sync_prts_operator_index;
    use super::sync_prts_recipe_index;
    use super::sync_prts_site_info;
    use super::sync_prts_stage_index;
    use crate::database::AppDatabase;
    use crate::database::default_database_path;
    use crate::official::OfficialNoticeClient;
    use crate::penguin::PenguinClient;
    use crate::prts::PrtsClient;
    use crate::repository::AppRepository;
    use crate::repository::ExternalEventNoticeUpsert;
    use crate::repository::ExternalItemDefUpsert;
    use crate::repository::PenguinItemUpsert;
    use crate::repository::PenguinMatrixUpsert;
    use crate::repository::PenguinStageUpsert;
    use crate::repository::RawSourceCacheUpsert;
    use serde_json::json;
    use std::fs;
    use std::io::ErrorKind;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn sync_official_notices_writes_cache_and_notice_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = official_notice_test_html();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("official-sync");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = OfficialNoticeClient::with_news_url(format!("http://{address}/news")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            repository
                .upsert_external_event_notices(&[ExternalEventNoticeUpsert {
                    notice_id: "old-non-event".to_string(),
                    title: "旧的非活动公告".to_string(),
                    notice_type: "notice".to_string(),
                    published_at: "2026-01-01T00:00:00+08:00".to_string(),
                    start_at: None,
                    end_at: None,
                    source_url: "https://ak.hypergryph.com/news/old-non-event".to_string(),
                    confirmed: true,
                    raw_json: json!({"title": "旧的非活动公告"}),
                }])
                .unwrap();
            let outcome = sync_official_notices(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, OFFICIAL_NOTICE_SOURCE_ID);
            assert_eq!(outcome.cache_key, OFFICIAL_NOTICE_CACHE_KEY);
            assert_eq!(outcome.row_count, 5);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, content_type FROM raw_source_cache WHERE cache_key = ?1",
                [OFFICIAL_NOTICE_CACHE_KEY],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(cache_row.0, "official");
        assert!(cache_row.1.contains("text/html"));

        let notice_row = database
            .connection()
            .query_row(
                "SELECT notice_type, published_at, start_at, end_at, confirmed
                 FROM external_event_notice
                 WHERE notice_id = '9697'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(notice_row.0, "activity");
        assert_eq!(notice_row.1, "2026-03-11T10:20:00+08:00");
        assert_eq!(notice_row.2, Some("2026-03-14T16:00:00+08:00".to_string()));
        assert_eq!(notice_row.3, Some("2026-04-25T03:59:00+08:00".to_string()));
        assert_eq!(notice_row.4, 1);

        let notice_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_event_notice", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(notice_count, 5);

        let stale_notice_count = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM external_event_notice WHERE notice_id = 'old-non-event'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(stale_notice_count, 0);

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [OFFICIAL_NOTICE_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_official_notice_failure_writes_failed_state_and_alert() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = "internal";
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("official-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = OfficialNoticeClient::with_news_url(format!("http://{address}/news")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_official_notices(&repository, &client).unwrap_err();
            assert!(error.to_string().contains("unexpected HTTP status"));
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [OFFICIAL_NOTICE_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{OFFICIAL_NOTICE_SOURCE_ID}")],
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
    fn sync_prts_item_index_writes_cache_and_item_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = prts_item_index_test_body();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("prts-item-index");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_prts_item_index(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PRTS_ITEM_INDEX_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_ITEM_INDEX_CACHE_KEY);
            assert_eq!(outcome.revision, "335500");
            assert_eq!(outcome.row_count, 2);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_ITEM_INDEX_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "335500");
        assert!(cache_row.2.contains("application/json"));

        let item_row = database
            .connection()
            .query_row(
                "SELECT name_zh, item_type, rarity
                 FROM external_item_def
                 WHERE item_id = '30104'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(item_row.0, "双极纳米片");
        assert_eq!(item_row.1, "养成材料");
        assert_eq!(item_row.2, Some(4));

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_ITEM_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_item_index_incremental_skips_when_revision_is_unchanged() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_for_server = Arc::clone(&request_count);

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 2048];
            let _ = stream.read(&mut request_buffer).unwrap();
            request_count_for_server.fetch_add(1, Ordering::SeqCst);
            let body = r#"{"parse":{"title":"道具一览","pageid":1109,"revid":335500}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("prts-item-index-skip");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            repository
                .upsert_external_item_defs(&[ExternalItemDefUpsert {
                    item_id: "4001".to_string(),
                    name_zh: "龙门币".to_string(),
                    item_type: "货币".to_string(),
                    rarity: Some(3),
                    raw_json: json!({"item_id": "4001"}),
                }])
                .unwrap();
            repository
                .upsert_raw_source_cache(&RawSourceCacheUpsert {
                    cache_key: PRTS_ITEM_INDEX_CACHE_KEY,
                    source_name: "prts",
                    revision: Some("335500"),
                    content_type: "application/json",
                    payload: br#"{"cached":true}"#,
                    expires_at: None,
                })
                .unwrap();
            repository
                .record_sync_success(PRTS_ITEM_INDEX_SOURCE_ID, Some("335500"))
                .unwrap();

            let outcome =
                sync_prts_item_index_with_mode(&repository, &client, SyncMode::Incremental)
                    .unwrap();
            assert_eq!(outcome.run_status, SyncRunStatus::SkippedUnchanged);
            assert_eq!(outcome.effective_mode, SyncMode::Incremental);
            assert_eq!(outcome.revision, "335500");
            assert_eq!(outcome.row_count, 1);
        }

        assert_eq!(request_count.load(Ordering::SeqCst), 1);

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_item_index_failure_writes_failed_state_and_alert() {
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

        let base_directory = unique_test_path("prts-item-index-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_item_index(&repository, &client).unwrap_err();
            assert!(!error.to_string().is_empty());
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_ITEM_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{PRTS_ITEM_INDEX_SOURCE_ID}")],
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
    fn sync_prts_operator_index_writes_cache_and_operator_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                let body = if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                    r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                } else {
                    prts_operator_index_test_body()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("prts-operator-index");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            repository
                .upsert_external_operator_defs(&[crate::repository::ExternalOperatorDefUpsert {
                    operator_id: "char_610_acfend".to_string(),
                    name_zh: "Mechanist(卫戍协议)".to_string(),
                    rarity: 5,
                    profession: "重装".to_string(),
                    branch: None,
                    server: "CN".to_string(),
                    raw_json: json!({
                        "operator_id": "char_610_acfend",
                        "availability_kind": "exclusive_mode",
                    }),
                }])
                .unwrap();
            let outcome = sync_prts_operator_index(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PRTS_OPERATOR_INDEX_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_OPERATOR_INDEX_CACHE_KEY);
            assert_eq!(outcome.revision, "335492");
            assert_eq!(outcome.row_count, 2);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_OPERATOR_INDEX_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "335492");
        assert!(cache_row.2.contains("application/json"));

        let operator_row = database
            .connection()
            .query_row(
                "SELECT name_zh, profession, branch, json_extract(raw_json, '$.page_title')
                 FROM external_operator_def
                 WHERE operator_id = 'char_002_amiya'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(operator_row.0, "阿米娅");
        assert_eq!(operator_row.1, "术师");
        assert_eq!(operator_row.2, Some("中坚术师".to_string()));
        assert_eq!(operator_row.3, "阿米娅");

        let filtered_count = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM external_operator_def WHERE operator_id IN (?1, ?2)",
                ["char_610_acfend", "char_513_apionr"],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(filtered_count, 0);

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_OPERATOR_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_operator_index_failure_writes_failed_state_and_alert() {
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

        let base_directory = unique_test_path("prts-operator-index-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_operator_index(&repository, &client).unwrap_err();
            assert!(!error.to_string().is_empty());
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_OPERATOR_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{PRTS_OPERATOR_INDEX_SOURCE_ID}")],
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
    fn sync_prts_operator_growth_writes_cache_and_growth_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..8 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                    r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#.to_string()
                } else if request.contains("%E5%B9%B2%E5%91%98id") {
                    prts_operator_growth_operator_index_test_body().to_string()
                } else if request.contains("page=12F&prop=sections%7Crevid") {
                    r#"{"parse":{"title":"12F","pageid":1703,"revid":400001,"sections":[{"line":"精英化材料","index":"9"},{"line":"技能升级材料","index":"10"}]}}"#.to_string()
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=sections%7Crevid")
                {
                    r#"{"parse":{"title":"能天使","pageid":1769,"revid":400002,"sections":[{"line":"精英化材料","index":"9"},{"line":"技能升级材料","index":"10"}]}}"#.to_string()
                } else if request.contains("page=12F&prop=text&section=9") {
                    r#"{"parse":{"title":"12F","pageid":1703,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><p>该干员无法精英化</p></div>"}}}"#.to_string()
                } else if request.contains("page=12F&prop=text&section=10") {
                    r#"{"parse":{"title":"12F","pageid":1703,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><p>该干员没有技能</p></div>"}}}"#.to_string()
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=text&section=9") {
                    prts_operator_growth_elite_section_test_body().to_string()
                } else {
                    prts_operator_growth_skill_section_test_body().to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("prts-operator-growth");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_prts_operator_growth(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PRTS_OPERATOR_GROWTH_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_OPERATOR_GROWTH_CACHE_KEY);
            assert_eq!(outcome.revision, "400002");
            assert_eq!(outcome.row_count, 4);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_OPERATOR_GROWTH_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "400002");
        assert!(cache_row.2.contains("application/json"));

        let growth_row = database
            .connection()
            .query_row(
                "SELECT
                    stage_label,
                    material_slot,
                    json_extract(raw_json, '$.materials[0].item_name_zh'),
                    json_extract(raw_json, '$.materials[0].count')
                 FROM external_operator_growth
                 WHERE growth_id = 'char_103_angel:elite_0_1:promotion'",
                [],
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
        assert_eq!(growth_row.0, "精英阶段0→1");
        assert_eq!(growth_row.1, "精英化");
        assert_eq!(growth_row.2, "龙门币");
        assert_eq!(growth_row.3, 30_000);

        let synced_operator_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_operator_def", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(synced_operator_count, 2);

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_OPERATOR_GROWTH_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_operator_growth_failure_writes_failed_state_and_alert() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let (status_line, body) =
                    if attempt == 2 && request.contains("page=12F&prop=sections%7Crevid") {
                        (
                            "HTTP/1.1 500 Internal Server Error",
                            r#"{"error":"internal"}"#.to_string(),
                        )
                    } else if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                        (
                            "HTTP/1.1 200 OK",
                            r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                                .to_string(),
                        )
                    } else {
                        (
                            "HTTP/1.1 200 OK",
                            prts_operator_growth_operator_index_test_body().to_string(),
                        )
                    };
                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("prts-operator-growth-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_operator_growth(&repository, &client).unwrap_err();
            assert!(!error.to_string().is_empty());
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_OPERATOR_GROWTH_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{PRTS_OPERATOR_GROWTH_SOURCE_ID}")],
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
    fn sync_prts_operator_building_skill_writes_cache_and_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..6 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                    r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#.to_string()
                } else if request.contains("%E5%B9%B2%E5%91%98id") {
                    prts_operator_building_skill_operator_index_test_body().to_string()
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=sections%7Crevid")
                {
                    r#"{"parse":{"title":"能天使","pageid":1769,"revid":400002,"sections":[{"line":"后勤技能","index":"8"}]}}"#.to_string()
                } else if request.contains("page=%E9%98%BF%E7%B1%B3%E5%A8%85&prop=sections%7Crevid")
                {
                    r#"{"parse":{"title":"阿米娅","pageid":1751,"revid":400003,"sections":[{"line":"后勤技能","index":"8"}]}}"#.to_string()
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=text&section=8") {
                    prts_operator_building_skill_angel_section_test_body().to_string()
                } else {
                    prts_operator_building_skill_amiya_section_test_body().to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("prts-operator-building-skill");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_prts_operator_building_skill(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY);
            assert_eq!(outcome.revision, "400003");
            assert_eq!(outcome.row_count, 4);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "400003");
        assert!(cache_row.2.contains("application/json"));

        let skill_row = database
            .connection()
            .query_row(
                "SELECT
                    room_type,
                    skill_name,
                    json_extract(raw_json, '$.condition_label'),
                    json_extract(raw_json, '$.room_type_label'),
                    json_extract(raw_json, '$.description')
                 FROM external_operator_building_skill
                 WHERE skill_id = 'char_103_angel:elite_0:trading_post:row1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(skill_row.0, "trading_post");
        assert_eq!(skill_row.1, "企鹅物流·α");
        assert_eq!(skill_row.2, "精英0");
        assert_eq!(skill_row.3, "贸易站");
        assert!(skill_row.4.contains("订单获取效率"));

        let synced_operator_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_operator_def", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(synced_operator_count, 2);

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_operator_building_skill_failure_writes_failed_state_and_alert() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let (status_line, body) = if attempt == 2
                    && request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=sections%7Crevid")
                {
                    (
                        "HTTP/1.1 500 Internal Server Error",
                        r#"{"error":"internal"}"#.to_string(),
                    )
                } else if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                    (
                        "HTTP/1.1 200 OK",
                        r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                            .to_string(),
                    )
                } else {
                    (
                        "HTTP/1.1 200 OK",
                        prts_operator_building_skill_operator_index_test_body().to_string(),
                    )
                };
                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("prts-operator-building-skill-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_operator_building_skill(&repository, &client).unwrap_err();
            assert!(!error.to_string().is_empty());
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!(
                    "sync-failure:{PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID}"
                )],
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
    fn sync_prts_stage_index_writes_cache_and_stage_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                let body = if request.contains("action=parse") {
                    r#"{"parse":{"title":"关卡一览","pageid":2325,"revid":375661}}"#
                } else {
                    prts_stage_index_test_body()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("prts-stage-index");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls(
            format!("http://{address}/api.php?action=query"),
            format!("http://{address}/items"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_prts_stage_index(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PRTS_STAGE_INDEX_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_STAGE_INDEX_CACHE_KEY);
            assert_eq!(outcome.revision, "375661");
            assert_eq!(outcome.row_count, 2);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_STAGE_INDEX_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "375661");
        assert!(cache_row.2.contains("application/json"));

        let stage_row = database
            .connection()
            .query_row(
                "SELECT code, json_extract(raw_json, '$.prts.page_title')
                 FROM external_stage_def
                 WHERE stage_id = 'main_01-07'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(stage_row.0, "1-7");
        assert_eq!(stage_row.1, "1-7 暴君");

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_STAGE_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_stage_index_failure_writes_failed_state_and_alert() {
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

        let base_directory = unique_test_path("prts-stage-index-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls(
            format!("http://{address}/api.php?action=query"),
            format!("http://{address}/items"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_stage_index(&repository, &client).unwrap_err();
            assert!(!error.to_string().is_empty());
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_STAGE_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{PRTS_STAGE_INDEX_SOURCE_ID}")],
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
    fn sync_prts_recipe_index_writes_cache_and_recipe_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 2048];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = prts_recipe_index_test_body();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("prts-recipe-index");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls_and_recipe(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
            format!("http://{address}/recipes"),
        )
        .unwrap();
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
                        item_id: "30035".to_string(),
                        name_zh: "研磨石".to_string(),
                        item_type: "养成材料".to_string(),
                        rarity: Some(1),
                        raw_json: json!({"item_id": "30035"}),
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

            let outcome = sync_prts_recipe_index(&repository, &client).unwrap();
            assert_eq!(outcome.source_id, PRTS_RECIPE_INDEX_SOURCE_ID);
            assert_eq!(outcome.cache_key, PRTS_RECIPE_INDEX_CACHE_KEY);
            assert_eq!(outcome.revision, "342715");
            assert_eq!(outcome.row_count, 1);
        }

        let cache_row = database
            .connection()
            .query_row(
                "SELECT source_name, revision, content_type
                 FROM raw_source_cache
                 WHERE cache_key = ?1",
                [PRTS_RECIPE_INDEX_CACHE_KEY],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(cache_row.0, "prts");
        assert_eq!(cache_row.1, "342715");
        assert!(cache_row.2.contains("application/json"));

        let recipe_row = database
            .connection()
            .query_row(
                "SELECT output_item_id, room_type, json_extract(raw_json, '$.ingredients[0].item_id')
                 FROM external_recipe
                 WHERE recipe_id = 'workshop:30033:lv3:row1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(recipe_row.0, "30033");
        assert_eq!(recipe_row.1, "workshop");
        assert_eq!(recipe_row.2, "30032");

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_RECIPE_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_prts_recipe_index_failure_writes_failed_state_and_alert() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 2048];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = prts_recipe_index_test_body();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let base_directory = unique_test_path("prts-recipe-index-failure");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls_and_recipe(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
            format!("http://{address}/recipes"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let error = sync_prts_recipe_index(&repository, &client).unwrap_err();
            assert!(error.to_string().contains("未匹配到 item_id"));
        }

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [PRTS_RECIPE_INDEX_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "failed");

        let alert = database
            .connection()
            .query_row(
                "SELECT alert_type, severity, status FROM alert WHERE alert_id = ?1",
                [format!("sync-failure:{PRTS_RECIPE_INDEX_SOURCE_ID}")],
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
    fn recipe_item_resolution_prefers_prts_definition_over_penguin_alias() {
        let base_directory = unique_test_path("prts-recipe-item-resolution");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        {
            let repository = AppRepository::new(database.connection());
            repository
                .upsert_external_item_defs(&[
                    ExternalItemDefUpsert {
                        item_id: "200008".to_string(),
                        name_zh: "碳".to_string(),
                        item_type: "建材原材料".to_string(),
                        rarity: Some(1),
                        raw_json: json!({
                            "item_id": "200008",
                            "name_zh": "碳",
                            "item_type": "建材原材料",
                            "categories": ["道具", "建材原材料"],
                        }),
                    },
                    ExternalItemDefUpsert {
                        item_id: "3112".to_string(),
                        name_zh: "碳".to_string(),
                        item_type: "MATERIAL".to_string(),
                        rarity: Some(1),
                        raw_json: json!({
                            "itemId": "3112",
                            "name": "碳",
                            "sortId": 200008,
                            "groupID": "carbon",
                        }),
                    },
                ])
                .unwrap();

            let item_id = resolve_external_item_id_by_name(&repository, "碳").unwrap();
            assert_eq!(item_id.as_deref(), Some("200008"));
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn sync_prts_runs_siteinfo_operator_item_stage_and_recipe_in_sequence() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();

        let server = thread::spawn(move || {
            let mut handled_requests = 0_usize;
            let mut idle_rounds = 0_u8;

            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        handled_requests += 1;
                        idle_rounds = 0;
                        stream.set_nonblocking(false).unwrap();

                        let mut request_buffer = [0_u8; 4096];
                        let bytes_read = stream.read(&mut request_buffer).unwrap();
                        let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                        let body = if request.contains("GET /siteinfo ") {
                            r#"{"query":{"general":{"sitename":"PRTS","generator":"MediaWiki 1.43.5","base":"https://prts.wiki/w/首页","server":"//prts.wiki","time":"2026-03-16T01:00:00Z"}}}"#.to_string()
                        } else if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                            r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                                .to_string()
                        } else if request.contains("%E5%B9%B2%E5%91%98id") {
                            prts_operator_growth_operator_index_test_body().to_string()
                        } else if request.contains("GET /items ") {
                            prts_item_index_with_recipe_items_test_body()
                        } else if request.contains("page=%E5%85%B3%E5%8D%A1%E4%B8%80%E8%A7%88") {
                            r#"{"parse":{"title":"关卡一览","pageid":2325,"revid":375661}}"#
                                .to_string()
                        } else if request.contains("GET /recipes ") {
                            prts_recipe_index_test_body()
                        } else {
                            prts_stage_index_test_body().to_string()
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream.write_all(response.as_bytes()).unwrap();
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        if handled_requests > 0 && idle_rounds >= 40 {
                            break;
                        }
                        idle_rounds = idle_rounds.saturating_add(1);
                        thread::sleep(Duration::from_millis(25));
                    }
                    Err(error) => panic!("unexpected listener error: {error}"),
                }
            }
        });

        let base_directory = unique_test_path("prts-all");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PrtsClient::with_urls_and_recipe(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
            format!("http://{address}/recipes"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            let outcome = sync_prts(&repository, &client, &base_directory).unwrap();
            assert_eq!(outcome.site_info.revision, "2026-03-16T01:00:00Z");
            assert_eq!(outcome.operator_index.row_count, 2);
            assert_eq!(outcome.item_index.row_count, 3);
            assert_eq!(outcome.stage_index.row_count, 2);
            assert_eq!(outcome.recipe_index.row_count, 1);
        }

        let operator_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_operator_def", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(operator_count, 2);

        let recipe_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_recipe", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(recipe_count, 1);

        let growth_count = database
            .connection()
            .query_row("SELECT COUNT(*) FROM external_operator_growth", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(growth_count, 0);

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn sync_penguin_matrix_writes_cache_and_drop_rows() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                let body = if request.contains("GET /matrix ") {
                    r#"{"matrix":[{"stageId":"main_01-07","itemId":"30011","times":100,"quantity":31,"stdDev":0.42,"start":1744012800000,"end":null},{"stageId":"main_01-07","itemId":"30012","times":100,"quantity":52,"stdDev":0.49,"start":1744012800000,"end":null}]}"#
                } else if request.contains("GET /stages ") {
                    r#"[{"stageId":"main_01-07","zoneId":"main_1","stageType":"MAIN","code":"1-7","apCost":6,"existence":{"CN":{"exist":true}}}]"#
                } else {
                    r#"[{"itemId":"30011","name":"源岩","itemType":"MATERIAL","rarity":0,"existence":{"CN":{"exist":true}}},{"itemId":"30012","name":"固源岩","itemType":"MATERIAL","rarity":1,"existence":{"CN":{"exist":true}}}]"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("penguin-sync");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PenguinClient::with_urls(
            format!("http://{address}/matrix"),
            format!("http://{address}/stages"),
            format!("http://{address}/items"),
        )
        .unwrap();
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

        let stage_row = database
            .connection()
            .query_row(
                "SELECT code FROM external_stage_def WHERE stage_id = 'main_01-07'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(stage_row, "1-7");

        let item_row = database
            .connection()
            .query_row(
                "SELECT name_zh FROM external_item_def WHERE item_id = '30012'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(item_row, "固源岩");

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

    #[test]
    fn sync_penguin_matrix_incremental_skips_when_last_modified_anchor_matches() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_for_server = Arc::clone(&request_count);
        let last_modified = "Mon, 16 Mar 2026 11:16:45 GMT";

        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                request_count_for_server.fetch_add(1, Ordering::SeqCst);

                let response = if request.starts_with("HEAD /matrix ")
                    || request.starts_with("HEAD /stages ")
                    || request.starts_with("HEAD /items ")
                {
                    format!(
                        "HTTP/1.1 200 OK\r\nLast-Modified: {last_modified}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                } else {
                    panic!("unexpected request during penguin incremental skip test: {request}");
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("penguin-skip");
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = PenguinClient::with_urls(
            format!("http://{address}/matrix"),
            format!("http://{address}/stages"),
            format!("http://{address}/items"),
        )
        .unwrap();
        {
            let repository = AppRepository::new(database.connection());
            repository
                .upsert_raw_source_cache(&RawSourceCacheUpsert {
                    cache_key: PENGUIN_MATRIX_CACHE_KEY,
                    source_name: "penguin",
                    revision: Some("1773115200000"),
                    content_type: "application/json",
                    payload: br#"{"matrix":[]}"#,
                    expires_at: None,
                })
                .unwrap();
            repository
                .replace_penguin_matrix(
                    &[PenguinMatrixUpsert {
                        matrix_id: "main_01-07:30011:1744012800000:0".to_string(),
                        stage_id: "main_01-07".to_string(),
                        item_id: "30011".to_string(),
                        sample_count: 100,
                        drop_count: 31,
                        window_start_at: Some("1744012800000".to_string()),
                        window_end_at: None,
                        raw_json: json!({}),
                    }],
                    &[PenguinStageUpsert {
                        stage_id: "main_01-07".to_string(),
                        zone_id: Some("main_1".to_string()),
                        code: "1-7".to_string(),
                        is_open: true,
                        raw_json: json!({}),
                    }],
                    &[PenguinItemUpsert {
                        item_id: "30011".to_string(),
                        name_zh: "源岩".to_string(),
                        item_type: "MATERIAL".to_string(),
                        rarity: Some(0),
                        raw_json: json!({}),
                    }],
                )
                .unwrap();
            repository
                .record_sync_success(
                    PENGUIN_MATRIX_SOURCE_ID,
                    Some(&format!(
                        "matrix={last_modified}|stages={last_modified}|items={last_modified}"
                    )),
                )
                .unwrap();

            let outcome =
                sync_penguin_matrix_with_mode(&repository, &client, SyncMode::Incremental).unwrap();
            assert_eq!(outcome.run_status, SyncRunStatus::SkippedUnchanged);
            assert_eq!(outcome.effective_mode, SyncMode::Incremental);
            assert_eq!(outcome.row_count, 1);
            assert_eq!(outcome.revision, "1773115200000");
        }

        assert_eq!(request_count.load(Ordering::SeqCst), 3);

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

    fn official_notice_test_html() -> String {
        let payload = serde_json::json!([
            "$",
            "$L18",
            serde_json::Value::Null,
            {
                "initialData": {
                    "NOTICE": {
                        "list": [{
                            "cid": "7373",
                            "tab": "0",
                            "sticky": false,
                            "title": "[明日方舟]03月14日16:00服务器停机维护公告",
                            "author": "【明日方舟】运营组",
                            "displayTime": 1773457200_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "感谢您对《明日方舟》的关注与支持，《明日方舟》计划将于2026年03月14日16:00 ~ 17:00开启服务器停机维护。"
                        }]
                    },
                    "ACTIVITY": {
                        "list": [{
                            "cid": "5114",
                            "tab": "1",
                            "sticky": false,
                            "title": "[明日方舟]「卫戍协议：盟约」更新公告",
                            "author": "【明日方舟】运营组",
                            "displayTime": 1773459000_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "感谢大家对《明日方舟》的关注与支持。「卫戍协议：盟约」下半期活动将于03月14日16:00 ~ 17:00的停机维护后开启。"
                        }, {
                            "cid": "9697",
                            "tab": "1",
                            "sticky": false,
                            "title": "[活动预告] 「卫戍协议：盟约」限时活动即将开启",
                            "author": "【明日方舟】运营组",
                            "displayTime": 1773195600_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "一、「卫戍协议：盟约」限时活动开启关卡开放时间：03月14日 16:00 - 04月25日 03:59解锁条件：通关主线1-10"
                        }, {
                            "cid": "8586",
                            "tab": "1",
                            "sticky": false,
                            "title": "「十字路口」创作征集活动",
                            "author": "【明日方舟】运营组",
                            "displayTime": 1773115200_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "「十字路口」创作征集进行中。《明日方舟》「十字路口」创作征集活动现已在多平台进行中。"
                        }]
                    },
                    "NEWS": {
                        "list": [{
                            "cid": "7600",
                            "tab": "2",
                            "sticky": false,
                            "title": "《明日方舟》制作组通讯#62期",
                            "author": "【明日方舟】制作组",
                            "displayTime": 1773028800_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "制作组通讯内容。"
                        }]
                    }
                }
            }
        ]);

        let encoded = serde_json::to_string(&format!("d:{payload}")).unwrap();
        format!(
            "<html><body><script>self.__next_f = self.__next_f || []; self.__next_f.push([1,{encoded}])</script></body></html>"
        )
    }

    fn prts_item_index_test_body() -> String {
        r#"{"parse":{"title":"道具一览","pageid":1109,"revid":335500,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\" lang=\"zh-Hans-CN\" dir=\"ltr\"><div class=\"smwdata\" data-name=\"龙门币\" data-description=\"基础货币。\" data-usage=\"用于养成。\" data-obtain_approach=\"&#91;&#91;主线&#93;&#93;掉落\" data-rarity=\"3\" data-category=\"分类:道具, 分类:货币\" data-id=\"4001\" data-dark-background=\"no\" data-file=\"https&#58;//media.prts.wiki/item_4001.png\"></div><div class=\"smwdata\" data-name=\"双极纳米片\" data-description=\"高阶材料。\" data-usage=\"用于精二与专精。\" data-obtain_approach=\"加工站合成\" data-rarity=\"4\" data-category=\"分类:道具, 分类:养成材料\" data-id=\"30104\" data-dark-background=\"yes\" data-file=\"https&#58;//media.prts.wiki/item_30104.png\"></div></div>"}}}"#.to_string()
    }

    fn prts_operator_index_test_body() -> &'static str {
        r#"{"query":{"results":{"12F":{"printouts":{"干员id":["char_009_12fce"],"稀有度":["1"],"职业":["术师"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"}]},"fulltext":"12F","fullurl":"//prts.wiki/w/12F"},"阿米娅":{"printouts":{"干员id":["char_002_amiya"],"稀有度":["5"],"职业":["术师"],"分支":[{"fulltext":"中坚术师"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"},{"fulltext":"分类:属于罗德岛的干员"}]},"fulltext":"阿米娅","fullurl":"//prts.wiki/w/%E9%98%BF%E7%B1%B3%E5%A8%85"},"Mechanist(卫戍协议)":{"printouts":{"干员id":["char_610_acfend"],"稀有度":["5"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"},{"fulltext":"分类:专属干员"}]},"fulltext":"Mechanist(卫戍协议)","fullurl":"//prts.wiki/w/Mechanist(%E5%8D%AB%E6%88%8D%E5%8D%8F%E8%AE%AE)"},"预备干员-重装":{"printouts":{"干员id":["char_513_apionr"],"稀有度":["3"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"}]},"fulltext":"预备干员-重装","fullurl":"//prts.wiki/w/%E9%A2%84%E5%A4%87%E5%B9%B2%E5%91%98-%E9%87%8D%E8%A3%85"}}}}"#
    }

    fn prts_operator_growth_operator_index_test_body() -> &'static str {
        r#"{"query":{"results":{"12F":{"printouts":{"干员id":["char_009_12fce"],"稀有度":["1"],"职业":["术师"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"}]},"fulltext":"12F","fullurl":"//prts.wiki/w/12F"},"能天使":{"printouts":{"干员id":["char_103_angel"],"稀有度":["5"],"职业":["狙击"],"分支":[{"fulltext":"速射手"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:狙击干员"}]},"fulltext":"能天使","fullurl":"//prts.wiki/w/%E8%83%BD%E5%A4%A9%E4%BD%BF"},"预备干员-重装":{"printouts":{"干员id":["char_513_apionr"],"稀有度":["3"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"}]},"fulltext":"预备干员-重装","fullurl":"//prts.wiki/w/%E9%A2%84%E5%A4%87%E5%B9%B2%E5%91%98-%E9%87%8D%E8%A3%85"}}}}"#
    }

    fn prts_operator_growth_elite_section_test_body() -> &'static str {
        r#"{"parse":{"title":"能天使","pageid":1769,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table><tbody><tr><th>精英阶段0→1</th><td><div><a title=\"龙门币\"></a><span>3w</span></div><div><a title=\"狙击芯片\"></a><span>5</span></div></td></tr></tbody></table></div>"}}}"#
    }

    fn prts_operator_growth_skill_section_test_body() -> &'static str {
        r#"{"parse":{"title":"能天使","pageid":1769,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table><tbody><tr><th colspan=\"2\">技能升级</th></tr><tr><th>1→2</th><td><div><a title=\"技巧概要·卷1\"></a><span>5</span></div></td></tr><tr><th colspan=\"2\">达到精英阶段1后解锁</th></tr><tr><th>4→5</th><td><div><a title=\"技巧概要·卷2\"></a><span>8</span></div></td></tr><tr><th colspan=\"2\">专精训练(达到精英阶段2后解锁)</th></tr><tr><th colspan=\"2\">第1技能</th></tr><tr><th>等级1</th><td><div><a title=\"技巧概要·卷3\"></a><span>6</span></div></td></tr></tbody></table></div>"}}}"#
    }

    fn prts_operator_building_skill_operator_index_test_body() -> &'static str {
        r#"{"query":{"results":{"能天使":{"printouts":{"干员id":["char_103_angel"],"稀有度":["5"],"职业":["狙击"],"分支":[{"fulltext":"速射手"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:狙击干员"}]},"fulltext":"能天使","fullurl":"//prts.wiki/w/%E8%83%BD%E5%A4%A9%E4%BD%BF"},"阿米娅":{"printouts":{"干员id":["char_002_amiya"],"稀有度":["5"],"职业":["术师"],"分支":[{"fulltext":"中坚术师"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"}]},"fulltext":"阿米娅","fullurl":"//prts.wiki/w/%E9%98%BF%E7%B1%B3%E5%A8%85"},"预备干员-重装":{"printouts":{"干员id":["char_513_apionr"],"稀有度":["3"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"}]},"fulltext":"预备干员-重装","fullurl":"//prts.wiki/w/%E9%A2%84%E5%A4%87%E5%B9%B2%E5%91%98-%E9%87%8D%E8%A3%85"}}}}"#
    }

    fn prts_operator_building_skill_angel_section_test_body() -> &'static str {
        r#"{"parse":{"title":"能天使","pageid":1769,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table class=\"wikitable logo\"><tbody><tr><th>条件</th><th>图标</th><th>技能1</th><th>房间</th><th>描述</th></tr><tr><td>精英0</td><td><img alt=\"企鹅物流·α\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_tra_spd1.png\" /></td><td>企鹅物流·α</td><td>贸易站</td><td>进驻贸易站时，订单获取效率+20%</td></tr><tr><td>精英2</td><td><img alt=\"物流专家\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_tra_spd3.png\" /></td><td>物流专家</td><td>贸易站</td><td>进驻贸易站时，订单获取效率+35%</td></tr></tbody></table></div>"}}}"#
    }

    fn prts_operator_building_skill_amiya_section_test_body() -> &'static str {
        r#"{"parse":{"title":"阿米娅","pageid":1751,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table class=\"wikitable logo\"><tbody><tr><th>条件</th><th>图标</th><th>技能1</th><th>房间</th><th>描述</th></tr><tr><td>精英0</td><td><img alt=\"合作协议\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_ctrl_t_spd.png\" /></td><td>合作协议</td><td>控制中枢</td><td>进驻控制中枢时，所有贸易站订单效率+7%</td></tr></tbody></table><table class=\"wikitable logo\"><tbody><tr><th>条件</th><th>图标</th><th>技能2</th><th>房间</th><th>描述</th></tr><tr><td>精英2</td><td><img alt=\"小提琴独奏\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_dorm_all2.png\" /></td><td>小提琴独奏</td><td>宿舍</td><td>进驻宿舍时，该宿舍内所有干员的心情每小时恢复+0.15</td></tr></tbody></table></div>"}}}"#
    }

    fn prts_item_index_with_recipe_items_test_body() -> String {
        r#"{"parse":{"title":"道具一览","pageid":1109,"revid":335500,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\" lang=\"zh-Hans-CN\" dir=\"ltr\"><div class=\"smwdata\" data-name=\"异铁\" data-description=\"基础材料。\" data-usage=\"用于合成。\" data-obtain_approach=\"主线掉落\" data-rarity=\"1\" data-category=\"分类:道具, 分类:养成材料\" data-id=\"30032\" data-dark-background=\"no\" data-file=\"https&#58;//media.prts.wiki/item_30032.png\"></div><div class=\"smwdata\" data-name=\"研磨石\" data-description=\"基础材料。\" data-usage=\"用于合成。\" data-obtain_approach=\"主线掉落\" data-rarity=\"1\" data-category=\"分类:道具, 分类:养成材料\" data-id=\"30035\" data-dark-background=\"no\" data-file=\"https&#58;//media.prts.wiki/item_30035.png\"></div><div class=\"smwdata\" data-name=\"异铁组\" data-description=\"进阶材料。\" data-usage=\"用于精二与专精。\" data-obtain_approach=\"加工站合成\" data-rarity=\"2\" data-category=\"分类:道具, 分类:养成材料\" data-id=\"30033\" data-dark-background=\"yes\" data-file=\"https&#58;//media.prts.wiki/item_30033.png\"></div></div>"}}}"#.to_string()
    }

    fn prts_recipe_index_test_body() -> String {
        r#"{"parse":{"title":"罗德岛基建/加工站","pageid":15788,"revid":342715,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\" lang=\"zh-Hans-CN\" dir=\"ltr\"><table class=\"wikitable logo\"><tbody><tr><th rowspan=\"2\">加工站等级</th><th rowspan=\"2\">所需原料</th><th rowspan=\"2\">产品</th><th colspan=\"2\">加工消耗</th><th rowspan=\"2\">副产物概率</th><th rowspan=\"2\">额外解锁条件</th></tr><tr><td>加工费用</td><td>心情消耗</td></tr><tr><th colspan=\"7\">精英材料</th></tr><tr><td>3</td><td><div style=\"display:inline-block;position:relative\"><span typeof=\"mw:File\"><a href=\"/w/%E5%BC%82%E9%93%81\" title=\"异铁\"><img src=\"https://media.prts.wiki/item_a.png\" /></a></span><span>3</span></div><div style=\"display:inline-block;position:relative\"><span typeof=\"mw:File\"><a href=\"/w/%E7%A0%94%E7%A3%A8%E7%9F%B3\" title=\"研磨石\"><img src=\"https://media.prts.wiki/item_b.png\" /></a></span><span>1</span></div></td><td><div style=\"display:inline-block;position:relative\"><span typeof=\"mw:File\"><a href=\"/w/%E5%BC%82%E9%93%81%E7%BB%84\" title=\"异铁组\"><img src=\"https://media.prts.wiki/item_c.png\" /></a></span><span>1</span></div></td><td>300</td><td>2</td><td>10%</td><td>－</td></tr></tbody></table></div>"}}}"#.to_string()
    }

    fn prts_stage_index_test_body() -> &'static str {
        r#"{"query":{"results":{"1-7 暴君":{"printouts":{"关卡id":["main_01-07"],"分类":[{"fulltext":"分类:主线关卡"},{"fulltext":"分类:普通难度关卡"}]},"fulltext":"1-7 暴君","fullurl":"//prts.wiki/w/1-7_%E6%9A%B4%E5%90%9B"},"CA-5 战略要道净空":{"printouts":{"关卡id":["wk_fly_5"],"分类":[{"fulltext":"分类:日常关卡"}]},"fulltext":"CA-5 战略要道净空","fullurl":"//prts.wiki/w/CA-5_%E6%88%98%E7%95%A5%E8%A6%81%E9%81%93%E5%87%80%E7%A9%BA"}}}}"#
    }
}
