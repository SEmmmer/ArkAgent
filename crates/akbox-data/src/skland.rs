use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hmac::Hmac;
use hmac::Mac;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sha2::Sha256;
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::repository::AppRepository;
use crate::repository::AuditLogEntry;
use crate::repository::BaseBuildingSnapshotInsert;
use crate::repository::BaseBuildingStateUpsert;
use crate::repository::ExternalOperatorDefRecord;
use crate::repository::OperatorSnapshotInsert;
use crate::repository::OperatorStateUpsert;
use crate::repository::PlayerStatusSnapshotInsert;
use crate::repository::PlayerStatusStateUpsert;
use crate::repository::RawSourceCacheUpsert;
use crate::repository::RepositoryError;

pub const DEFAULT_SKLAND_API_BASE_URL: &str = "https://zonai.skland.com/api/v1";
pub const DEFAULT_SKLAND_AUTH_FILE_NAME: &str = "skland-auth.local.toml";
pub const SKLAND_PLAYER_INFO_SOURCE_ID: &str = "skland.player-info.current";
pub const SKLAND_PLAYER_INFO_CACHE_KEY: &str = "skland:player-info:current";

const SKLAND_USER_AGENT: &str =
    "Skland/1.32.1 (com.hypergryph.skland; build:103201004; Android 33; ) Okhttp/4.11.0";

#[derive(Debug, Clone)]
pub struct SklandClient {
    http_client: Client,
    api_base_url: String,
}

impl SklandClient {
    pub fn new() -> Result<Self, SklandClientError> {
        Self::with_api_base_url(DEFAULT_SKLAND_API_BASE_URL)
    }

    pub fn with_api_base_url(api_base_url: impl Into<String>) -> Result<Self, SklandClientError> {
        let http_client = Client::builder()
            .user_agent(SKLAND_USER_AGENT)
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|source| SklandClientError::BuildHttpClient { source })?;

        Ok(Self {
            http_client,
            api_base_url: api_base_url.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SklandProfileRequest {
    pub auth_file_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SklandOperatorSample {
    pub operator_id: String,
    pub name_zh: String,
    pub level: i64,
    pub elite_stage: i64,
    pub skill_level: i64,
    pub mastery_1: i64,
    pub mastery_2: i64,
    pub mastery_3: i64,
    pub module_state: Option<String>,
    pub module_level: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SklandPlayerInfoInspectOutcome {
    pub source_id: String,
    pub cache_key: String,
    pub revision: String,
    pub cache_size_bytes: usize,
    pub uid: String,
    pub account_name: Option<String>,
    pub status_store_ts: Option<i64>,
    pub status_keys: Vec<String>,
    pub binding_count: usize,
    pub char_count: usize,
    pub assist_count: usize,
    pub equipment_info_count: usize,
    pub char_info_count: usize,
    pub has_building: bool,
    pub building_keys: Vec<String>,
    pub has_control: bool,
    pub has_meeting: bool,
    pub has_training: bool,
    pub has_hire: bool,
    pub dormitory_count: usize,
    pub manufacture_count: usize,
    pub trading_count: usize,
    pub power_count: usize,
    pub tired_char_count: usize,
    pub sample_operator: Option<SklandOperatorSample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SklandOperatorImportOutcome {
    pub inspect: SklandPlayerInfoInspectOutcome,
    pub snapshot_id: String,
    pub imported_row_count: usize,
    pub owned_row_count: usize,
    pub unowned_row_count: usize,
    pub used_external_operator_defs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SklandStatusBuildingImportOutcome {
    pub inspect: SklandPlayerInfoInspectOutcome,
    pub player_status_snapshot_id: String,
    pub base_building_snapshot_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SklandPlayerInfo {
    #[serde(default)]
    pub status: SklandPlayerStatus,
    #[serde(default, rename = "assistChars")]
    pub assist_chars: Vec<SklandCharacter>,
    #[serde(default)]
    pub chars: Vec<SklandCharacter>,
    #[serde(default)]
    pub building: serde_json::Value,
    #[serde(default, rename = "equipmentInfoMap")]
    pub equipment_info_map: HashMap<String, serde_json::Value>,
    #[serde(default, rename = "charInfoMap")]
    pub char_info_map: HashMap<String, SklandCharacterInfo>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct SklandPlayerStatus {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "storeTs")]
    pub store_ts: Option<i64>,
    #[serde(default, flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SklandCharacter {
    #[serde(rename = "charId")]
    pub char_id: String,
    #[serde(default)]
    pub level: i64,
    #[serde(default, rename = "evolvePhase")]
    pub evolve_phase: i64,
    #[serde(default, rename = "mainSkillLvl")]
    pub main_skill_lvl: i64,
    #[serde(default)]
    pub skills: Vec<SklandSkillState>,
    #[serde(default, deserialize_with = "deserialize_skland_equip_list")]
    pub equip: Vec<SklandEquipState>,
    #[serde(default, rename = "defaultEquipId")]
    pub default_equip_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SklandSkillState {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "specializeLevel")]
    pub specialize_level: i64,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SklandEquipState {
    pub id: String,
    #[serde(default)]
    pub level: i64,
    #[serde(default)]
    pub locked: bool,
}

fn deserialize_skland_equip_list<'de, D>(deserializer: D) -> Result<Vec<SklandEquipState>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum EquipField {
        List(Vec<SklandEquipState>),
        Single(SklandEquipState),
        Empty(serde_json::Value),
    }

    match Option::<EquipField>::deserialize(deserializer)? {
        None => Ok(Vec::new()),
        Some(EquipField::List(values)) => Ok(values),
        Some(EquipField::Single(value)) => Ok(vec![value]),
        Some(EquipField::Empty(serde_json::Value::Null)) => Ok(Vec::new()),
        Some(EquipField::Empty(_)) => Ok(Vec::new()),
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SklandCharacterInfo {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub profession: Option<String>,
    #[serde(default)]
    pub rarity: Option<i64>,
    #[serde(default, rename = "subProfessionName")]
    pub sub_profession_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SklandBindingData {
    #[serde(default)]
    list: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SklandApiEnvelope<T> {
    status: Option<i64>,
    code: Option<i64>,
    msg: Option<String>,
    message: Option<String>,
    data: Option<T>,
}

impl<T> SklandApiEnvelope<T> {
    fn status_code(&self) -> Option<i64> {
        self.code.or(self.status)
    }

    fn message_text(&self) -> String {
        self.message
            .as_deref()
            .or(self.msg.as_deref())
            .unwrap_or("服务端没有返回可读消息")
            .to_string()
    }
}

#[derive(Debug, Clone)]
struct SklandResolvedAuth {
    uid: String,
    cred: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct SklandRefreshTokenData {
    token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct SklandLocalAuthFile {
    #[serde(default)]
    skland: SklandLocalAuthSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SklandLocalAuthSection {
    #[serde(default = "default_uid_file_name")]
    uid_file: String,
    #[serde(default)]
    cred: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    access_token: String,
}

impl Default for SklandLocalAuthSection {
    fn default() -> Self {
        Self {
            uid_file: default_uid_file_name(),
            cred: String::new(),
            token: String::new(),
            user_id: String::new(),
            access_token: String::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SklandClientError {
    #[error("failed to build skland HTTP client: {source}")]
    BuildHttpClient { source: reqwest::Error },
    #[error("failed to read skland auth file {path}: {source}")]
    ReadAuthFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse skland auth file {path}: {source}")]
    ParseAuthFile {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to read skland uid file {path}: {source}")]
    ReadUidFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write skland auth file {path}: {source}")]
    WriteAuthFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("skland auth file {path} does not contain a non-empty cred")]
    MissingCred { path: PathBuf },
    #[error("skland auth file {path} does not contain a non-empty token")]
    MissingToken { path: PathBuf },
    #[error("skland uid file {path} does not contain a non-empty uid")]
    MissingUid { path: PathBuf },
    #[error("failed to build skland signed request URL: {message}")]
    BuildUrl { message: String },
    #[error("failed to send skland request {url}: {source}")]
    SendRequest { url: String, source: reqwest::Error },
    #[error("skland request {url} returned an unsuccessful status: {source}")]
    HttpStatus { url: String, source: reqwest::Error },
    #[error("failed to read skland response body from {url}: {source}")]
    ReadResponseBody { url: String, source: reqwest::Error },
    #[error("failed to decode skland response body from {url}: {source}")]
    DecodeResponseBody {
        url: String,
        source: serde_json::Error,
    },
    #[error("failed to sign skland request: {message}")]
    SignRequest { message: String },
    #[error("{operation} failed: {message}")]
    ApiError {
        operation: &'static str,
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum SklandPlayerInfoError {
    #[error(transparent)]
    Client(#[from] SklandClientError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("failed to serialize skland payload: {source}")]
    SerializeJson { source: serde_json::Error },
}

fn default_uid_file_name() -> String {
    "uid.txt".to_string()
}

pub fn discover_default_skland_auth_file(start_directory: &Path) -> Option<PathBuf> {
    for ancestor in start_directory.ancestors() {
        for candidate in [
            ancestor.join(DEFAULT_SKLAND_AUTH_FILE_NAME),
            ancestor
                .join("target")
                .join("debug")
                .join(DEFAULT_SKLAND_AUTH_FILE_NAME),
            ancestor
                .join("target")
                .join("release")
                .join(DEFAULT_SKLAND_AUTH_FILE_NAME),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn load_skland_auth(auth_file_path: &Path) -> Result<SklandResolvedAuth, SklandClientError> {
    let document =
        fs::read_to_string(auth_file_path).map_err(|source| SklandClientError::ReadAuthFile {
            path: auth_file_path.to_path_buf(),
            source,
        })?;
    let parsed = toml::from_str::<SklandLocalAuthFile>(&document).map_err(|source| {
        SklandClientError::ParseAuthFile {
            path: auth_file_path.to_path_buf(),
            source,
        }
    })?;

    let cred = parsed.skland.cred.trim().to_string();
    if cred.is_empty() {
        return Err(SklandClientError::MissingCred {
            path: auth_file_path.to_path_buf(),
        });
    }

    let token = parsed.skland.token.trim().to_string();
    if token.is_empty() {
        return Err(SklandClientError::MissingToken {
            path: auth_file_path.to_path_buf(),
        });
    }

    let uid_file = resolve_uid_file_path(auth_file_path, parsed.skland.uid_file.trim());
    let uid = fs::read_to_string(&uid_file).map_err(|source| SklandClientError::ReadUidFile {
        path: uid_file.clone(),
        source,
    })?;
    let uid = uid.trim().to_string();
    if uid.is_empty() {
        return Err(SklandClientError::MissingUid { path: uid_file });
    }

    Ok(SklandResolvedAuth { uid, cred, token })
}

fn persist_skland_token(auth_file_path: &Path, token: &str) -> Result<(), SklandClientError> {
    let document =
        fs::read_to_string(auth_file_path).map_err(|source| SklandClientError::ReadAuthFile {
            path: auth_file_path.to_path_buf(),
            source,
        })?;
    let mut parsed = toml::from_str::<SklandLocalAuthFile>(&document).map_err(|source| {
        SklandClientError::ParseAuthFile {
            path: auth_file_path.to_path_buf(),
            source,
        }
    })?;
    parsed.skland.token = token.to_string();
    let serialized =
        toml::to_string_pretty(&parsed).map_err(|source| SklandClientError::ApiError {
            operation: "更新森空岛本地 token",
            message: format!("序列化本地鉴权文件失败：{source}"),
        })?;
    fs::write(auth_file_path, serialized).map_err(|source| SklandClientError::WriteAuthFile {
        path: auth_file_path.to_path_buf(),
        source,
    })
}

fn resolve_uid_file_path(auth_file_path: &Path, uid_file_value: &str) -> PathBuf {
    let uid_path = PathBuf::from(uid_file_value);
    if uid_path.is_absolute() {
        return uid_path;
    }

    let auth_directory = auth_file_path.parent().unwrap_or_else(|| Path::new("."));
    let direct_path = auth_directory.join(&uid_path);
    if direct_path.exists() {
        return direct_path;
    }

    for ancestor in auth_directory.ancestors() {
        let candidate = ancestor.join(&uid_path);
        if candidate.exists() {
            return candidate;
        }
    }

    direct_path
}

impl SklandClient {
    fn refresh_signing_token(&self, cred: &str) -> Result<String, SklandClientError> {
        let request_url = format!("{}/auth/refresh", self.api_base_url);
        let response = self
            .http_client
            .get(&request_url)
            .header("Accept-Encoding", "gzip")
            .header("Connection", "close")
            .header("cred", cred)
            .send()
            .map_err(|source| SklandClientError::SendRequest {
                url: request_url.clone(),
                source,
            })?
            .error_for_status()
            .map_err(|source| SklandClientError::HttpStatus {
                url: request_url.clone(),
                source,
            })?;

        let body = response
            .text()
            .map_err(|source| SklandClientError::ReadResponseBody {
                url: request_url.clone(),
                source,
            })?;
        let normalized_body = normalize_skland_response_text(&body);
        let envelope =
            serde_json::from_str::<SklandApiEnvelope<SklandRefreshTokenData>>(normalized_body)
                .map_err(|source| SklandClientError::ApiError {
                    operation: "刷新森空岛签名 token",
                    message: format!(
                        "解析响应失败：{source}；响应摘要：{}",
                        summarize_skland_response_body(normalized_body)
                    ),
                })?;
        let data = require_skland_success_data(envelope, "刷新森空岛签名 token")?;
        Ok(data.token)
    }

    fn fetch_binding_count(&self, auth: &SklandResolvedAuth) -> Result<usize, SklandClientError> {
        let (_, envelope) = self.signed_get_json::<SklandBindingData>(
            auth,
            "/game/player/binding",
            &[],
            "查询森空岛角色绑定",
        )?;
        let data = require_skland_success_data(envelope, "查询森空岛角色绑定")?;
        Ok(data.list.len())
    }

    fn fetch_player_info(
        &self,
        auth: &SklandResolvedAuth,
    ) -> Result<(Vec<u8>, SklandPlayerInfo), SklandClientError> {
        let (raw_body, envelope) = self.signed_get_json::<SklandPlayerInfo>(
            auth,
            "/game/player/info",
            &[("uid", auth.uid.as_str())],
            "查询森空岛 player/info",
        )?;
        let data = require_skland_success_data(envelope, "查询森空岛 player/info")?;
        Ok((raw_body, data))
    }

    fn signed_get_json<T: for<'de> Deserialize<'de>>(
        &self,
        auth: &SklandResolvedAuth,
        path: &str,
        query: &[(&str, &str)],
        operation: &'static str,
    ) -> Result<(Vec<u8>, SklandApiEnvelope<T>), SklandClientError> {
        let query_string = encode_query_string(query);
        let request_url = if query_string.is_empty() {
            format!("{}{}", self.api_base_url, path)
        } else {
            format!("{}{}?{}", self.api_base_url, path, query_string)
        };
        let parsed_url =
            reqwest::Url::parse(&request_url).map_err(|source| SklandClientError::BuildUrl {
                message: source.to_string(),
            })?;
        let timestamp = (OffsetDateTime::now_utc().unix_timestamp() - 1).to_string();
        let header_json = format!(
            "{{\"platform\":\"\",\"timestamp\":\"{timestamp}\",\"dId\":\"\",\"vName\":\"\"}}"
        );
        let sign_secret = build_skland_sign_secret(
            parsed_url.path(),
            parsed_url.query().unwrap_or(""),
            &timestamp,
            &header_json,
        );
        let sign = compute_skland_sign(&auth.token, &sign_secret)?;

        let response = self
            .http_client
            .get(&request_url)
            .header("Accept-Encoding", "gzip")
            .header("Connection", "close")
            .header("cred", auth.cred.as_str())
            .header("sign", sign)
            .header("platform", "")
            .header("timestamp", timestamp)
            .header("dId", "")
            .header("vName", "")
            .send()
            .map_err(|source| SklandClientError::SendRequest {
                url: request_url.clone(),
                source,
            })?
            .error_for_status()
            .map_err(|source| SklandClientError::HttpStatus {
                url: request_url.clone(),
                source,
            })?;

        let body = response
            .text()
            .map_err(|source| SklandClientError::ReadResponseBody {
                url: request_url.clone(),
                source,
            })?;
        let normalized_body = normalize_skland_response_text(&body);
        let raw_body = normalized_body.as_bytes().to_vec();
        let envelope =
            serde_json::from_str::<SklandApiEnvelope<T>>(normalized_body).map_err(|source| {
                SklandClientError::ApiError {
                    operation,
                    message: format!(
                        "解析响应失败：{source}；响应摘要：{}",
                        summarize_skland_response_body(normalized_body)
                    ),
                }
            })?;

        if !matches!(envelope.status_code(), Some(0)) {
            return Err(SklandClientError::ApiError {
                operation,
                message: envelope.message_text(),
            });
        }

        Ok((raw_body, envelope))
    }
}

fn build_skland_sign_secret(
    path: &str,
    query_string: &str,
    timestamp: &str,
    header_json: &str,
) -> String {
    format!("{path}{query_string}{timestamp}{header_json}")
}

fn require_skland_success_data<T>(
    envelope: SklandApiEnvelope<T>,
    operation: &'static str,
) -> Result<T, SklandClientError> {
    envelope.data.ok_or_else(|| SklandClientError::ApiError {
        operation,
        message: "服务端返回成功，但 data 为空".to_string(),
    })
}

fn encode_query_string(query: &[(&str, &str)]) -> String {
    query
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn compute_skland_sign(token: &str, secret: &str) -> Result<String, SklandClientError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(token.as_bytes()).map_err(|error| {
        SklandClientError::SignRequest {
            message: error.to_string(),
        }
    })?;
    mac.update(secret.as_bytes());
    let digest = mac.finalize().into_bytes();
    let hex_secret = digest
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect::<String>();
    Ok(format!("{:x}", md5::compute(hex_secret.as_bytes())))
}

struct SklandFetchedPlayerBundle {
    uid: String,
    binding_count: usize,
    raw_body: Vec<u8>,
    player_info: SklandPlayerInfo,
    revision: String,
}

fn normalize_skland_response_text(body: &str) -> &str {
    body.trim_start_matches('\u{feff}')
}

fn summarize_skland_response_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "响应体为空".to_string();
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let mut parts = Vec::new();
        if let Some(status) = value.get("status").and_then(|value| value.as_i64()) {
            parts.push(format!("status={status}"));
        }
        if let Some(code) = value.get("code").and_then(|value| value.as_i64()) {
            parts.push(format!("code={code}"));
        }
        if let Some(msg) = value.get("msg").and_then(|value| value.as_str()) {
            parts.push(format!("msg={msg}"));
        }
        if let Some(message) = value.get("message").and_then(|value| value.as_str()) {
            parts.push(format!("message={message}"));
        }
        if let Some(data) = value.get("data") {
            match data {
                serde_json::Value::Object(map) => {
                    let mut keys = map.keys().cloned().collect::<Vec<_>>();
                    keys.sort();
                    parts.push(format!("data_keys={}", keys.join(",")));
                }
                serde_json::Value::Array(values) => {
                    parts.push(format!("data_len={}", values.len()));
                }
                serde_json::Value::Null => parts.push("data=null".to_string()),
                _ => parts.push("data=<scalar>".to_string()),
            }
        }

        if !parts.is_empty() {
            return parts.join("；");
        }
    }

    format!("响应体不是可解析 JSON，长度 {} 字节", trimmed.len())
}

pub fn inspect_skland_player_info(
    repository: &AppRepository<'_>,
    client: &SklandClient,
    request: &SklandProfileRequest,
) -> Result<SklandPlayerInfoInspectOutcome, SklandPlayerInfoError> {
    let fetched = fetch_and_cache_skland_player_info(repository, client, request)?;
    Ok(build_inspect_outcome(&fetched))
}

pub fn import_skland_player_info_into_status_and_building_state(
    repository: &AppRepository<'_>,
    client: &SklandClient,
    request: &SklandProfileRequest,
) -> Result<SklandStatusBuildingImportOutcome, SklandPlayerInfoError> {
    let fetched = fetch_and_cache_skland_player_info(repository, client, request)?;
    let inspect = build_inspect_outcome(&fetched);
    let status_snapshot_id = format!(
        "skland-player-status-snapshot-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    let building_snapshot_id = format!(
        "skland-base-building-snapshot-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    let status_plan = build_player_status_plan(
        fetched.uid.as_str(),
        &fetched.player_info.status,
        &status_snapshot_id,
    )?;
    let building_plan = build_base_building_plan(
        fetched.uid.as_str(),
        &fetched.player_info.building,
        &building_snapshot_id,
    );

    repository.replace_player_status_and_base_building_snapshot(
        &status_plan.snapshot,
        &status_plan.state,
        &building_plan.snapshot,
        &building_plan.state,
    )?;

    let audit_payload = json!({
        "uid": fetched.uid,
        "revision": inspect.revision.clone(),
        "player_status_snapshot_id": status_snapshot_id.clone(),
        "base_building_snapshot_id": building_snapshot_id.clone(),
        "account_name": inspect.account_name.clone(),
        "status_keys": inspect.status_keys.clone(),
        "building_keys": inspect.building_keys.clone(),
        "dormitory_count": inspect.dormitory_count,
        "manufacture_count": inspect.manufacture_count,
        "trading_count": inspect.trading_count,
        "power_count": inspect.power_count,
        "tired_char_count": inspect.tired_char_count,
    });
    let audit_payload_json = serde_json::to_string(&audit_payload)
        .map_err(|source| SklandPlayerInfoError::SerializeJson { source })?;
    let audit_id = format!(
        "audit-skland-status-building-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    repository.append_audit_log(&AuditLogEntry {
        audit_id: audit_id.as_str(),
        entity_type: "player_status_snapshot",
        entity_id: Some(status_snapshot_id.as_str()),
        action: "import",
        summary: "森空岛 player/info 导入账号状态与基建当前态",
        payload_json: Some(audit_payload_json.as_str()),
        source: "skland.player-info.current",
    })?;

    Ok(SklandStatusBuildingImportOutcome {
        inspect,
        player_status_snapshot_id: status_snapshot_id,
        base_building_snapshot_id: building_snapshot_id,
    })
}

pub fn import_skland_player_info_into_operator_state(
    repository: &AppRepository<'_>,
    client: &SklandClient,
    request: &SklandProfileRequest,
) -> Result<SklandOperatorImportOutcome, SklandPlayerInfoError> {
    let fetched = fetch_and_cache_skland_player_info(repository, client, request)?;
    let inspect = build_inspect_outcome(&fetched);
    let imported_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
    let snapshot_id = format!(
        "skland-operator-snapshot-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    let plan =
        build_operator_state_plan(repository, &fetched.player_info, &snapshot_id, &imported_at)?;

    repository.replace_operator_state_snapshot(
        &OperatorSnapshotInsert {
            snapshot_id: snapshot_id.clone(),
            source: SKLAND_PLAYER_INFO_SOURCE_ID.to_string(),
            confidence: Some(1.0),
            note: Some(format!(
                "森空岛 player/info 导入：已持有 {} / 总计 {}",
                plan.owned_row_count, plan.imported_row_count
            )),
        },
        plan.entries.as_slice(),
    )?;

    let audit_id = format!(
        "audit-skland-operator-import-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    let payload_json = serde_json::to_string(&json!({
        "snapshot_id": snapshot_id.as_str(),
        "uid": inspect.uid.as_str(),
        "imported_row_count": plan.imported_row_count,
        "owned_row_count": plan.owned_row_count,
        "unowned_row_count": plan.unowned_row_count,
        "used_external_operator_defs": plan.used_external_operator_defs,
        "revision": inspect.revision.as_str(),
    }))
    .map_err(|source| SklandPlayerInfoError::SerializeJson { source })?;
    repository.append_audit_log(&AuditLogEntry {
        audit_id: audit_id.as_str(),
        entity_type: "operator_snapshot",
        entity_id: Some(snapshot_id.as_str()),
        action: "replace_from_skland",
        summary: "森空岛 player/info 导入干员当前态",
        payload_json: Some(payload_json.as_str()),
        source: SKLAND_PLAYER_INFO_SOURCE_ID,
    })?;

    Ok(SklandOperatorImportOutcome {
        inspect,
        snapshot_id,
        imported_row_count: plan.imported_row_count,
        owned_row_count: plan.owned_row_count,
        unowned_row_count: plan.unowned_row_count,
        used_external_operator_defs: plan.used_external_operator_defs,
    })
}

fn fetch_and_cache_skland_player_info(
    repository: &AppRepository<'_>,
    client: &SklandClient,
    request: &SklandProfileRequest,
) -> Result<SklandFetchedPlayerBundle, SklandPlayerInfoError> {
    repository.record_sync_attempt(SKLAND_PLAYER_INFO_SOURCE_ID)?;

    let fetched = (|| -> Result<SklandFetchedPlayerBundle, SklandPlayerInfoError> {
        let mut auth = load_skland_auth(request.auth_file_path.as_path())?;
        let refreshed_token = client.refresh_signing_token(auth.cred.as_str())?;
        if auth.token != refreshed_token {
            persist_skland_token(request.auth_file_path.as_path(), refreshed_token.as_str())?;
            auth.token = refreshed_token;
        }
        let binding_count = client.fetch_binding_count(&auth)?;
        let (raw_body, player_info) = client.fetch_player_info(&auth)?;
        let revision = player_info
            .status
            .store_ts
            .map(|value| value.to_string())
            .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp_nanos().to_string());

        repository.upsert_raw_source_cache(&RawSourceCacheUpsert {
            cache_key: SKLAND_PLAYER_INFO_CACHE_KEY,
            source_name: SKLAND_PLAYER_INFO_SOURCE_ID,
            revision: Some(revision.as_str()),
            content_type: "application/json",
            payload: raw_body.as_slice(),
            expires_at: None,
        })?;
        repository.record_sync_success(SKLAND_PLAYER_INFO_SOURCE_ID, Some(revision.as_str()))?;

        Ok(SklandFetchedPlayerBundle {
            uid: auth.uid,
            binding_count,
            raw_body,
            player_info,
            revision,
        })
    })();

    if let Err(error) = fetched.as_ref() {
        let _ = repository.record_sync_failure(SKLAND_PLAYER_INFO_SOURCE_ID, &error.to_string());
    }

    fetched
}

fn build_inspect_outcome(fetched: &SklandFetchedPlayerBundle) -> SklandPlayerInfoInspectOutcome {
    let building_keys = build_building_keys(&fetched.player_info.building);
    let status_keys = build_status_keys(&fetched.player_info.status);

    SklandPlayerInfoInspectOutcome {
        source_id: SKLAND_PLAYER_INFO_SOURCE_ID.to_string(),
        cache_key: SKLAND_PLAYER_INFO_CACHE_KEY.to_string(),
        revision: fetched.revision.clone(),
        cache_size_bytes: fetched.raw_body.len(),
        uid: fetched.uid.clone(),
        account_name: fetched
            .player_info
            .status
            .name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        status_store_ts: fetched.player_info.status.store_ts,
        status_keys,
        binding_count: fetched.binding_count,
        char_count: fetched.player_info.chars.len(),
        assist_count: fetched.player_info.assist_chars.len(),
        equipment_info_count: fetched.player_info.equipment_info_map.len(),
        char_info_count: fetched.player_info.char_info_map.len(),
        has_building: !fetched.player_info.building.is_null(),
        building_keys,
        has_control: json_has_non_null_key(&fetched.player_info.building, "control"),
        has_meeting: json_has_non_null_key(&fetched.player_info.building, "meeting"),
        has_training: json_has_non_null_key(&fetched.player_info.building, "training"),
        has_hire: json_has_non_null_key(&fetched.player_info.building, "hire"),
        dormitory_count: json_array_len(&fetched.player_info.building, "dormitories"),
        manufacture_count: json_array_len(&fetched.player_info.building, "manufactures"),
        trading_count: json_array_len(&fetched.player_info.building, "tradings"),
        power_count: json_array_len(&fetched.player_info.building, "powers"),
        tired_char_count: json_array_len(&fetched.player_info.building, "tiredChars"),
        sample_operator: fetched
            .player_info
            .chars
            .first()
            .map(|operator| build_operator_sample(operator, &fetched.player_info.char_info_map)),
    }
}

fn build_status_keys(status: &SklandPlayerStatus) -> Vec<String> {
    let mut keys = status.extra_fields.keys().cloned().collect::<Vec<_>>();
    if status.name.is_some() {
        keys.push("name".to_string());
    }
    if status.store_ts.is_some() {
        keys.push("storeTs".to_string());
    }
    keys.sort();
    keys.dedup();
    keys
}

fn build_building_keys(building: &serde_json::Value) -> Vec<String> {
    let mut keys = building
        .as_object()
        .map(|value| value.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn json_has_non_null_key(value: &serde_json::Value, key: &str) -> bool {
    value
        .as_object()
        .and_then(|map| map.get(key))
        .is_some_and(|value| !value.is_null())
}

fn json_array_len(value: &serde_json::Value, key: &str) -> usize {
    value
        .as_object()
        .and_then(|map| map.get(key))
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn build_operator_sample(
    operator: &SklandCharacter,
    char_info_map: &HashMap<String, SklandCharacterInfo>,
) -> SklandOperatorSample {
    let info = char_info_map.get(operator.char_id.as_str());
    let (module_state, module_level) = resolve_module_state(operator);

    SklandOperatorSample {
        operator_id: operator.char_id.clone(),
        name_zh: info
            .and_then(|value| value.name.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| operator.char_id.clone()),
        level: operator.level.max(1),
        elite_stage: operator.evolve_phase.max(0),
        skill_level: operator.main_skill_lvl.max(1),
        mastery_1: operator
            .skills
            .first()
            .map(|value| value.specialize_level)
            .unwrap_or(0),
        mastery_2: operator
            .skills
            .get(1)
            .map(|value| value.specialize_level)
            .unwrap_or(0),
        mastery_3: operator
            .skills
            .get(2)
            .map(|value| value.specialize_level)
            .unwrap_or(0),
        module_state,
        module_level,
    }
}

struct OperatorStateImportPlan {
    entries: Vec<OperatorStateUpsert>,
    imported_row_count: usize,
    owned_row_count: usize,
    unowned_row_count: usize,
    used_external_operator_defs: bool,
}

struct PlayerStatusImportPlan {
    snapshot: PlayerStatusSnapshotInsert,
    state: PlayerStatusStateUpsert,
}

struct BaseBuildingImportPlan {
    snapshot: BaseBuildingSnapshotInsert,
    state: BaseBuildingStateUpsert,
}

fn build_operator_state_plan(
    repository: &AppRepository<'_>,
    player_info: &SklandPlayerInfo,
    snapshot_id: &str,
    imported_at: &str,
) -> Result<OperatorStateImportPlan, SklandPlayerInfoError> {
    let external_operator_defs = repository.list_external_operator_defs(4096)?;
    let external_def_map = external_operator_defs
        .iter()
        .map(|entry| (entry.operator_id.as_str(), entry))
        .collect::<HashMap<_, _>>();
    let owned_operator_map = player_info
        .chars
        .iter()
        .map(|entry| (entry.char_id.as_str(), entry))
        .collect::<HashMap<_, _>>();

    let mut candidate_operator_ids = if !external_operator_defs.is_empty() {
        external_operator_defs
            .iter()
            .map(|entry| entry.operator_id.clone())
            .collect::<Vec<_>>()
    } else if !player_info.char_info_map.is_empty() {
        player_info
            .char_info_map
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    } else {
        player_info
            .chars
            .iter()
            .map(|entry| entry.char_id.clone())
            .collect::<Vec<_>>()
    };
    candidate_operator_ids.sort();
    candidate_operator_ids.dedup();

    let mut entries = Vec::with_capacity(candidate_operator_ids.len());
    let mut owned_row_count = 0_usize;

    for operator_id in candidate_operator_ids {
        let external_def = external_def_map.get(operator_id.as_str()).copied();
        let char_info = player_info.char_info_map.get(operator_id.as_str());
        let owned_state = owned_operator_map.get(operator_id.as_str()).copied();
        let owned = owned_state.is_some();
        if owned {
            owned_row_count += 1;
        }

        let (module_state, module_level) = owned_state
            .map(resolve_module_state)
            .unwrap_or((None, None));
        entries.push(OperatorStateUpsert {
            operator_id: operator_id.clone(),
            name_zh: resolve_operator_name(operator_id.as_str(), external_def, char_info),
            owned,
            rarity: resolve_operator_rarity(external_def, char_info),
            profession: resolve_operator_profession(external_def, char_info),
            branch: resolve_operator_branch(external_def, char_info),
            elite_stage: owned_state
                .map(|value| value.evolve_phase.max(0))
                .unwrap_or(0),
            level: owned_state.map(|value| value.level.max(1)).unwrap_or(1),
            skill_level: owned_state
                .map(|value| value.main_skill_lvl.max(1))
                .unwrap_or(1),
            mastery_1: owned_state
                .and_then(|value| value.skills.first())
                .map(|value| value.specialize_level)
                .unwrap_or(0),
            mastery_2: owned_state
                .and_then(|value| value.skills.get(1))
                .map(|value| value.specialize_level)
                .unwrap_or(0),
            mastery_3: owned_state
                .and_then(|value| value.skills.get(2))
                .map(|value| value.specialize_level)
                .unwrap_or(0),
            module_state,
            module_level,
            recognition_confidence: Some(1.0),
            last_scanned_at: Some(imported_at.to_string()),
            snapshot_id: snapshot_id.to_string(),
        });
    }

    let imported_row_count = entries.len();
    Ok(OperatorStateImportPlan {
        imported_row_count,
        owned_row_count,
        unowned_row_count: imported_row_count.saturating_sub(owned_row_count),
        entries,
        used_external_operator_defs: !external_operator_defs.is_empty(),
    })
}

fn build_player_status_plan(
    uid: &str,
    status: &SklandPlayerStatus,
    snapshot_id: &str,
) -> Result<PlayerStatusImportPlan, SklandPlayerInfoError> {
    let status_keys = build_status_keys(status);
    let raw_json = serde_json::to_value(status)
        .map_err(|source| SklandPlayerInfoError::SerializeJson { source })?;
    let account_name = status
        .name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(PlayerStatusImportPlan {
        snapshot: PlayerStatusSnapshotInsert {
            snapshot_id: snapshot_id.to_string(),
            source: SKLAND_PLAYER_INFO_SOURCE_ID.to_string(),
            uid: uid.to_string(),
            account_name: account_name.clone(),
            store_ts: status.store_ts,
            status_keys_json: json!(status_keys),
            raw_json: raw_json.clone(),
        },
        state: PlayerStatusStateUpsert {
            uid: uid.to_string(),
            account_name,
            store_ts: status.store_ts,
            status_keys_json: json!(build_status_keys(status)),
            snapshot_id: snapshot_id.to_string(),
            raw_json,
        },
    })
}

fn build_base_building_plan(
    uid: &str,
    building: &serde_json::Value,
    snapshot_id: &str,
) -> BaseBuildingImportPlan {
    let building_keys = build_building_keys(building);
    let has_control = json_has_non_null_key(building, "control");
    let has_meeting = json_has_non_null_key(building, "meeting");
    let has_training = json_has_non_null_key(building, "training");
    let has_hire = json_has_non_null_key(building, "hire");
    let dormitory_count = json_array_len(building, "dormitories") as i64;
    let manufacture_count = json_array_len(building, "manufactures") as i64;
    let trading_count = json_array_len(building, "tradings") as i64;
    let power_count = json_array_len(building, "powers") as i64;
    let tired_char_count = json_array_len(building, "tiredChars") as i64;

    BaseBuildingImportPlan {
        snapshot: BaseBuildingSnapshotInsert {
            snapshot_id: snapshot_id.to_string(),
            source: SKLAND_PLAYER_INFO_SOURCE_ID.to_string(),
            uid: uid.to_string(),
            has_control,
            has_meeting,
            has_training,
            has_hire,
            dormitory_count,
            manufacture_count,
            trading_count,
            power_count,
            tired_char_count,
            building_keys_json: json!(building_keys),
            raw_json: building.clone(),
        },
        state: BaseBuildingStateUpsert {
            uid: uid.to_string(),
            has_control,
            has_meeting,
            has_training,
            has_hire,
            dormitory_count,
            manufacture_count,
            trading_count,
            power_count,
            tired_char_count,
            building_keys_json: json!(build_building_keys(building)),
            snapshot_id: snapshot_id.to_string(),
            raw_json: building.clone(),
        },
    }
}

fn resolve_operator_name(
    operator_id: &str,
    external_def: Option<&ExternalOperatorDefRecord>,
    char_info: Option<&SklandCharacterInfo>,
) -> String {
    external_def
        .map(|value| value.name_zh.clone())
        .or_else(|| {
            char_info
                .and_then(|value| value.name.as_ref())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| operator_id.to_string())
}

fn resolve_operator_profession(
    external_def: Option<&ExternalOperatorDefRecord>,
    char_info: Option<&SklandCharacterInfo>,
) -> String {
    external_def
        .map(|value| value.profession.clone())
        .or_else(|| {
            char_info
                .and_then(|value| value.profession.as_ref())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "未知".to_string())
}

fn resolve_operator_branch(
    external_def: Option<&ExternalOperatorDefRecord>,
    char_info: Option<&SklandCharacterInfo>,
) -> Option<String> {
    external_def
        .and_then(|value| value.branch.clone())
        .or_else(|| {
            char_info
                .and_then(|value| value.sub_profession_name.as_ref())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn resolve_operator_rarity(
    external_def: Option<&ExternalOperatorDefRecord>,
    char_info: Option<&SklandCharacterInfo>,
) -> i64 {
    external_def
        .map(|value| value.rarity)
        .or_else(|| char_info.and_then(|value| value.rarity.map(|rarity| rarity + 1)))
        .unwrap_or(1)
        .max(1)
}

fn resolve_module_state(operator: &SklandCharacter) -> (Option<String>, Option<i64>) {
    if let Some(default_equip_id) = operator.default_equip_id.as_deref()
        && let Some(equip) = operator
            .equip
            .iter()
            .find(|entry| entry.id == default_equip_id && !entry.locked)
    {
        return (Some(equip.id.clone()), Some(equip.level.max(1)));
    }

    if let Some(equip) = operator.equip.iter().find(|entry| !entry.locked) {
        return (Some(equip.id.clone()), Some(equip.level.max(1)));
    }

    if operator.equip.is_empty() {
        (None, None)
    } else {
        (Some("locked".to_string()), None)
    }
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_SKLAND_AUTH_FILE_NAME;
    use super::SKLAND_PLAYER_INFO_CACHE_KEY;
    use super::SKLAND_PLAYER_INFO_SOURCE_ID;
    use super::SklandCharacter;
    use super::SklandClient;
    use super::SklandProfileRequest;
    use super::build_skland_sign_secret;
    use super::discover_default_skland_auth_file;
    use super::import_skland_player_info_into_operator_state;
    use super::import_skland_player_info_into_status_and_building_state;
    use super::inspect_skland_player_info;
    use super::resolve_uid_file_path;
    use crate::database::AppDatabase;
    use crate::database::default_database_path;
    use crate::repository::AppRepository;
    use crate::repository::ExternalOperatorDefUpsert;
    use serde_json::json;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_UID: &str = "test_uid_0001";
    const SECONDARY_TEST_UID: &str = "test_uid_0002";

    #[test]
    fn discover_default_skland_auth_file_prefers_existing_target_debug_file() {
        let base_directory = unique_test_path("discover-auth");
        let auth_file_path = base_directory
            .join("target")
            .join("debug")
            .join(DEFAULT_SKLAND_AUTH_FILE_NAME);
        fs::create_dir_all(auth_file_path.parent().unwrap()).unwrap();
        fs::write(&auth_file_path, "[skland]\nuid_file = \"uid.txt\"\n").unwrap();

        let discovered = discover_default_skland_auth_file(base_directory.as_path());
        assert_eq!(discovered, Some(auth_file_path.clone()));

        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn build_skland_sign_secret_omits_question_mark_before_query() {
        let secret = build_skland_sign_secret(
            "/api/v1/game/player/info",
            &format!("uid={TEST_UID}"),
            "1234567890",
            "{\"platform\":\"\",\"timestamp\":\"1234567890\",\"dId\":\"\",\"vName\":\"\"}",
        );

        assert_eq!(
            secret,
            format!(
                "/api/v1/game/player/infouid={TEST_UID}1234567890{{\"platform\":\"\",\"timestamp\":\"1234567890\",\"dId\":\"\",\"vName\":\"\"}}"
            )
        );
    }

    #[test]
    fn skland_character_accepts_single_object_equip_field() {
        let character = serde_json::from_value::<SklandCharacter>(json!({
            "charId": "char_4133_logos",
            "level": 90,
            "evolvePhase": 2,
            "mainSkillLvl": 7,
            "skills": [],
            "equip": {
                "id": "uniequip_4133_logos",
                "level": 3,
                "locked": false
            },
            "defaultEquipId": "uniequip_4133_logos"
        }))
        .unwrap();

        assert_eq!(character.equip.len(), 1);
        assert_eq!(character.equip[0].id, "uniequip_4133_logos");
        assert_eq!(character.equip[0].level, 3);
        assert!(!character.equip[0].locked);
    }

    #[test]
    fn inspect_skland_player_info_fetches_and_caches_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for expected_path in [
                "/api/v1/auth/refresh",
                "/api/v1/game/player/binding",
                &format!("/api/v1/game/player/info?uid={TEST_UID}"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                assert!(request.contains(expected_path));

                let body = if expected_path.contains("auth/refresh") {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": {
                            "token": "refreshed-signing-token"
                        }
                    })
                } else if expected_path.contains("binding") {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": {
                            "list": [{ "uid": TEST_UID }, { "uid": SECONDARY_TEST_UID }]
                        }
                    })
                } else {
                    sample_player_info_body()
                };
                let body = serde_json::to_string(&body).unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("inspect-player-info");
        let auth_file_path = write_sample_auth_files(base_directory.as_path());
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = SklandClient::with_api_base_url(format!("http://{address}/api/v1")).unwrap();
        let outcome = {
            let repository = AppRepository::new(database.connection());
            inspect_skland_player_info(
                &repository,
                &client,
                &SklandProfileRequest { auth_file_path },
            )
            .unwrap()
        };

        assert_eq!(outcome.binding_count, 2);
        assert_eq!(outcome.char_count, 1);
        assert_eq!(outcome.assist_count, 1);
        assert_eq!(outcome.equipment_info_count, 2);
        assert_eq!(outcome.char_info_count, 2);
        assert_eq!(outcome.status_store_ts, Some(1710600000));
        assert_eq!(
            outcome.status_keys,
            vec!["name".to_string(), "storeTs".to_string()]
        );
        assert!(outcome.has_building);
        assert_eq!(
            outcome.building_keys,
            vec!["chars".to_string(), "rooms".to_string()]
        );
        assert!(!outcome.has_control);
        assert!(!outcome.has_meeting);
        assert_eq!(outcome.dormitory_count, 0);
        assert_eq!(outcome.manufacture_count, 0);
        assert_eq!(outcome.trading_count, 0);
        assert_eq!(outcome.power_count, 0);
        assert_eq!(outcome.tired_char_count, 0);
        assert_eq!(outcome.account_name.as_deref(), Some("测试博士"));
        assert_eq!(
            outcome
                .sample_operator
                .as_ref()
                .map(|value| value.name_zh.as_str()),
            Some("阿米娅")
        );

        let cache = database
            .connection()
            .query_row(
                "SELECT source_name, revision FROM raw_source_cache WHERE cache_key = ?1",
                [SKLAND_PLAYER_INFO_CACHE_KEY],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(cache.0, SKLAND_PLAYER_INFO_SOURCE_ID);
        assert_eq!(cache.1, "1710600000");
        let persisted_auth =
            fs::read_to_string(base_directory.join(DEFAULT_SKLAND_AUTH_FILE_NAME)).unwrap();
        assert!(persisted_auth.contains("refreshed-signing-token"));

        let sync_status = database
            .connection()
            .query_row(
                "SELECT status FROM sync_source_state WHERE source_id = ?1",
                [SKLAND_PLAYER_INFO_SOURCE_ID],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(sync_status, "succeeded");

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn resolve_uid_file_path_walks_up_from_target_debug_auth_location() {
        let base_directory = unique_test_path("resolve-uid");
        let auth_file_path = base_directory
            .join("target")
            .join("debug")
            .join(DEFAULT_SKLAND_AUTH_FILE_NAME);
        fs::create_dir_all(auth_file_path.parent().unwrap()).unwrap();
        fs::write(base_directory.join("uid.txt"), format!("{TEST_UID}\n")).unwrap();

        let resolved = resolve_uid_file_path(auth_file_path.as_path(), "uid.txt");
        assert_eq!(resolved, base_directory.join("uid.txt"));

        fs::remove_dir_all(base_directory).unwrap();
    }

    #[test]
    fn import_skland_player_info_writes_operator_snapshot_and_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for expected_path in [
                "/api/v1/auth/refresh",
                "/api/v1/game/player/binding",
                &format!("/api/v1/game/player/info?uid={TEST_UID}"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                assert!(request.contains(expected_path));

                let body = if expected_path.contains("auth/refresh") {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": { "token": "refreshed-signing-token" }
                    })
                } else if expected_path.contains("binding") {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": { "list": [{ "uid": TEST_UID }] }
                    })
                } else {
                    sample_player_info_body()
                };
                let body = serde_json::to_string(&body).unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("import-player-info");
        let auth_file_path = write_sample_auth_files(base_directory.as_path());
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = SklandClient::with_api_base_url(format!("http://{address}/api/v1")).unwrap();

        {
            let repository = AppRepository::new(database.connection());
            repository
                .replace_external_operator_defs(&[
                    ExternalOperatorDefUpsert {
                        operator_id: "char_002_amiya".to_string(),
                        name_zh: "阿米娅".to_string(),
                        rarity: 5,
                        profession: "术师".to_string(),
                        branch: Some("中坚术师".to_string()),
                        server: "CN".to_string(),
                        raw_json: json!({ "operator_id": "char_002_amiya" }),
                    },
                    ExternalOperatorDefUpsert {
                        operator_id: "char_103_angel".to_string(),
                        name_zh: "能天使".to_string(),
                        rarity: 6,
                        profession: "狙击".to_string(),
                        branch: Some("速射手".to_string()),
                        server: "CN".to_string(),
                        raw_json: json!({ "operator_id": "char_103_angel" }),
                    },
                ])
                .unwrap();

            let outcome = import_skland_player_info_into_operator_state(
                &repository,
                &client,
                &SklandProfileRequest { auth_file_path },
            )
            .unwrap();

            assert_eq!(outcome.imported_row_count, 2);
            assert_eq!(outcome.owned_row_count, 1);
            assert_eq!(outcome.unowned_row_count, 1);
            assert!(outcome.used_external_operator_defs);
            assert!(outcome.snapshot_id.starts_with("skland-operator-snapshot-"));
        }

        let persisted_auth =
            fs::read_to_string(base_directory.join(DEFAULT_SKLAND_AUTH_FILE_NAME)).unwrap();
        assert!(persisted_auth.contains("refreshed-signing-token"));

        {
            let repository = AppRepository::new(database.connection());
            assert_eq!(repository.count_operator_snapshots().unwrap(), 1);
            assert_eq!(repository.count_operator_states().unwrap(), 2);

            let states = repository.list_operator_states(8).unwrap();
            assert_eq!(states[0].operator_id, "char_002_amiya");
            assert!(states[0].owned);
            assert_eq!(states[0].name_zh, "阿米娅");
            assert_eq!(states[0].rarity, 5);
            assert_eq!(states[0].elite_stage, 2);
            assert_eq!(states[0].level, 90);
            assert_eq!(states[0].skill_level, 7);
            assert_eq!(states[0].mastery_1, 3);
            assert_eq!(
                states[0].module_state.as_deref(),
                Some("uniequip_002_amiya")
            );
            assert_eq!(states[0].module_level, Some(2));

            assert_eq!(states[1].operator_id, "char_103_angel");
            assert!(!states[1].owned);
            assert_eq!(states[1].name_zh, "能天使");
            assert_eq!(states[1].rarity, 6);
            assert_eq!(states[1].level, 1);
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn import_skland_player_info_writes_player_status_and_base_building_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for expected_path in [
                "/api/v1/auth/refresh",
                "/api/v1/game/player/binding",
                &format!("/api/v1/game/player/info?uid={TEST_UID}"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);
                assert!(request.contains(expected_path));

                let body = if expected_path.contains("auth/refresh") {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": { "token": "refreshed-signing-token" }
                    })
                } else if expected_path.contains("binding") {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": { "list": [{ "uid": TEST_UID }] }
                    })
                } else {
                    json!({
                        "code": 0,
                        "message": "OK",
                        "data": {
                            "status": {
                                "name": "测试博士",
                                "storeTs": 1710600000
                            },
                            "assistChars": [],
                            "chars": [],
                            "building": {
                                "control": {},
                                "meeting": {},
                                "training": {},
                                "hire": {},
                                "dormitories": [{}, {}, {}, {}],
                                "manufactures": [{}, {}, {}],
                                "tradings": [{}, {}],
                                "powers": [{}, {}, {}],
                                "tiredChars": [{}, {}]
                            },
                            "equipmentInfoMap": {},
                            "charInfoMap": {}
                        }
                    })
                };
                let body = serde_json::to_string(&body).unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let base_directory = unique_test_path("import-status-building");
        let auth_file_path = write_sample_auth_files(base_directory.as_path());
        let database = AppDatabase::open(default_database_path(&base_directory)).unwrap();
        let client = SklandClient::with_api_base_url(format!("http://{address}/api/v1")).unwrap();

        {
            let repository = AppRepository::new(database.connection());
            let outcome = import_skland_player_info_into_status_and_building_state(
                &repository,
                &client,
                &SklandProfileRequest { auth_file_path },
            )
            .unwrap();

            assert!(
                outcome
                    .player_status_snapshot_id
                    .starts_with("skland-player-status-snapshot-")
            );
            assert!(
                outcome
                    .base_building_snapshot_id
                    .starts_with("skland-base-building-snapshot-")
            );
            assert_eq!(outcome.inspect.account_name.as_deref(), Some("测试博士"));
            assert_eq!(outcome.inspect.status_store_ts, Some(1710600000));
            assert!(outcome.inspect.has_control);
            assert!(outcome.inspect.has_meeting);
            assert!(outcome.inspect.has_training);
            assert!(outcome.inspect.has_hire);
            assert_eq!(outcome.inspect.dormitory_count, 4);
            assert_eq!(outcome.inspect.manufacture_count, 3);
            assert_eq!(outcome.inspect.trading_count, 2);
            assert_eq!(outcome.inspect.power_count, 3);
            assert_eq!(outcome.inspect.tired_char_count, 2);
        }

        let persisted_auth =
            fs::read_to_string(base_directory.join(DEFAULT_SKLAND_AUTH_FILE_NAME)).unwrap();
        assert!(persisted_auth.contains("refreshed-signing-token"));

        {
            let repository = AppRepository::new(database.connection());
            assert_eq!(repository.count_player_status_snapshots().unwrap(), 1);
            assert_eq!(repository.count_player_status_states().unwrap(), 1);
            assert_eq!(repository.count_base_building_snapshots().unwrap(), 1);
            assert_eq!(repository.count_base_building_states().unwrap(), 1);

            let player_status_states = repository.list_player_status_states(4).unwrap();
            assert_eq!(player_status_states.len(), 1);
            assert_eq!(
                player_status_states[0].account_name.as_deref(),
                Some("测试博士")
            );
            assert_eq!(player_status_states[0].store_ts, Some(1710600000));

            let base_building_states = repository.list_base_building_states(4).unwrap();
            assert_eq!(base_building_states.len(), 1);
            assert!(base_building_states[0].has_control);
            assert_eq!(base_building_states[0].dormitory_count, 4);
            assert_eq!(base_building_states[0].manufacture_count, 3);
            assert_eq!(base_building_states[0].trading_count, 2);
            assert_eq!(base_building_states[0].power_count, 3);
            assert_eq!(base_building_states[0].tired_char_count, 2);
        }

        drop(database);
        fs::remove_dir_all(base_directory).unwrap();
        server.join().unwrap();
    }

    fn write_sample_auth_files(base_directory: &Path) -> PathBuf {
        fs::create_dir_all(base_directory).unwrap();
        let auth_file_path = base_directory.join(DEFAULT_SKLAND_AUTH_FILE_NAME);
        fs::write(base_directory.join("uid.txt"), format!("{TEST_UID}\n")).unwrap();
        fs::write(
            &auth_file_path,
            "[skland]\nuid_file = \"uid.txt\"\ncred = \"test-cred\"\ntoken = \"test-token\"\n",
        )
        .unwrap();
        auth_file_path
    }

    fn sample_player_info_body() -> serde_json::Value {
        json!({
            "code": 0,
            "message": "OK",
            "data": {
                "status": {
                    "name": "测试博士",
                    "storeTs": 1710600000
                },
                "assistChars": [{
                    "charId": "char_103_angel",
                    "level": 80,
                    "evolvePhase": 2,
                    "mainSkillLvl": 7,
                    "skills": [
                        {"specializeLevel": 3},
                        {"specializeLevel": 3},
                        {"specializeLevel": 3}
                    ],
                    "equip": [],
                    "defaultEquipId": null
                }],
                "chars": [{
                    "charId": "char_002_amiya",
                    "level": 90,
                    "evolvePhase": 2,
                    "mainSkillLvl": 7,
                    "skills": [
                        {"specializeLevel": 3},
                        {"specializeLevel": 1},
                        {"specializeLevel": 0}
                    ],
                    "equip": [
                        {"id": "uniequip_002_amiya", "level": 2, "locked": false},
                        {"id": "uniequip_002_amiya_locked", "level": 1, "locked": true}
                    ],
                    "defaultEquipId": "uniequip_002_amiya"
                }],
                "building": {
                    "rooms": [],
                    "chars": []
                },
                "equipmentInfoMap": {
                    "uniequip_002_amiya": { "name": "测试模组" },
                    "uniequip_002_amiya_locked": { "name": "测试模组 2" }
                },
                "charInfoMap": {
                    "char_002_amiya": {
                        "name": "阿米娅",
                        "profession": "术师",
                        "rarity": 4,
                        "subProfessionName": "中坚术师"
                    },
                    "char_103_angel": {
                        "name": "能天使",
                        "profession": "狙击",
                        "rarity": 5,
                        "subProfessionName": "速射手"
                    }
                }
            }
        })
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "arkagent-skland-{label}-{}-{nanos}",
            std::process::id()
        ))
    }
}
