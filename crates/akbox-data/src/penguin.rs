use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

pub const DEFAULT_PENGUIN_MATRIX_URL: &str =
    "https://penguin-stats.io/PenguinStats/api/v2/result/matrix?server=CN";
pub const DEFAULT_PENGUIN_STAGES_URL: &str =
    "https://penguin-stats.io/PenguinStats/api/v2/stages?server=CN";
pub const DEFAULT_PENGUIN_ITEMS_URL: &str =
    "https://penguin-stats.io/PenguinStats/api/v2/items?server=CN";
const PENGUIN_FETCH_MAX_ATTEMPTS: usize = 3;
const PENGUIN_FETCH_RETRY_DELAYS_MS: [u64; 2] = [500, 1_500];

#[derive(Debug, Clone)]
pub struct PenguinClient {
    http_client: Client,
    matrix_url: String,
    stages_url: String,
    items_url: String,
}

impl PenguinClient {
    pub fn new() -> Result<Self, PenguinClientError> {
        Self::with_urls(
            DEFAULT_PENGUIN_MATRIX_URL,
            DEFAULT_PENGUIN_STAGES_URL,
            DEFAULT_PENGUIN_ITEMS_URL,
        )
    }

    pub fn with_matrix_url(matrix_url: impl Into<String>) -> Result<Self, PenguinClientError> {
        Self::with_urls(
            matrix_url,
            DEFAULT_PENGUIN_STAGES_URL,
            DEFAULT_PENGUIN_ITEMS_URL,
        )
    }

    pub fn with_urls(
        matrix_url: impl Into<String>,
        stages_url: impl Into<String>,
        items_url: impl Into<String>,
    ) -> Result<Self, PenguinClientError> {
        let http_client = Client::builder()
            .user_agent("ArkAgent/0.1 (https://github.com/openai/codex)")
            .build()
            .map_err(|source| PenguinClientError::BuildHttpClient { source })?;

        Ok(Self {
            http_client,
            matrix_url: matrix_url.into(),
            stages_url: stages_url.into(),
            items_url: items_url.into(),
        })
    }

    pub fn fetch_cn_matrix(&self) -> Result<PenguinMatrixResponse, PenguinClientError> {
        let (parsed, content_type, raw_body) =
            self.fetch_json_with_retries::<PenguinMatrixEnvelope>(&self.matrix_url, "matrix")?;

        Ok(PenguinMatrixResponse {
            rows: parsed.matrix,
            content_type,
            raw_body,
        })
    }

    pub fn fetch_cn_stages(&self) -> Result<PenguinStageIndexResponse, PenguinClientError> {
        let (stages, content_type, raw_body) =
            self.fetch_json_with_retries::<Vec<PenguinStage>>(&self.stages_url, "stages")?;

        Ok(PenguinStageIndexResponse {
            stages,
            content_type,
            raw_body,
        })
    }

    pub fn fetch_cn_items(&self) -> Result<PenguinItemIndexResponse, PenguinClientError> {
        let (items, content_type, raw_body) =
            self.fetch_json_with_retries::<Vec<PenguinItem>>(&self.items_url, "items")?;

        Ok(PenguinItemIndexResponse {
            items,
            content_type,
            raw_body,
        })
    }

    pub fn fetch_cn_matrix_last_modified(&self) -> Result<Option<String>, PenguinClientError> {
        self.fetch_last_modified_with_retries(&self.matrix_url, "matrix")
    }

    pub fn fetch_cn_stages_last_modified(&self) -> Result<Option<String>, PenguinClientError> {
        self.fetch_last_modified_with_retries(&self.stages_url, "stages")
    }

    pub fn fetch_cn_items_last_modified(&self) -> Result<Option<String>, PenguinClientError> {
        self.fetch_last_modified_with_retries(&self.items_url, "items")
    }

    fn fetch_json_with_retries<T>(
        &self,
        url: &str,
        endpoint_label: &str,
    ) -> Result<(T, String, Vec<u8>), PenguinClientError>
    where
        T: DeserializeOwned,
    {
        for attempt in 0..PENGUIN_FETCH_MAX_ATTEMPTS {
            match self.fetch_json_once::<T>(url) {
                Ok(response) => return Ok(response),
                Err(error) if error.is_retryable() && attempt + 1 < PENGUIN_FETCH_MAX_ATTEMPTS => {
                    let delay_ms = retry_delay_ms(attempt);
                    tracing::warn!(
                        endpoint = endpoint_label,
                        attempt = attempt + 1,
                        max_attempts = PENGUIN_FETCH_MAX_ATTEMPTS,
                        delay_ms,
                        error = %error,
                        "penguin request failed, retrying"
                    );
                    thread::sleep(Duration::from_millis(delay_ms));
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("retry loop should have returned or errored")
    }

    fn fetch_last_modified_with_retries(
        &self,
        url: &str,
        endpoint_label: &str,
    ) -> Result<Option<String>, PenguinClientError> {
        for attempt in 0..PENGUIN_FETCH_MAX_ATTEMPTS {
            match self.fetch_last_modified_once(url) {
                Ok(value) => return Ok(value),
                Err(error) if error.is_retryable() && attempt + 1 < PENGUIN_FETCH_MAX_ATTEMPTS => {
                    let delay_ms = retry_delay_ms(attempt);
                    tracing::warn!(
                        endpoint = endpoint_label,
                        attempt = attempt + 1,
                        max_attempts = PENGUIN_FETCH_MAX_ATTEMPTS,
                        delay_ms,
                        error = %error,
                        "penguin HEAD request failed, retrying"
                    );
                    thread::sleep(Duration::from_millis(delay_ms));
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("retry loop should have returned or errored")
    }

    fn fetch_json_once<T>(&self, url: &str) -> Result<(T, String, Vec<u8>), PenguinClientError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http_client
            .get(url)
            .send()
            .map_err(|source| PenguinClientError::SendRequest { source })?
            .error_for_status()
            .map_err(|source| PenguinClientError::HttpStatus { source })?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let raw_body = response
            .bytes()
            .map_err(|source| PenguinClientError::ReadResponseBody { source })?
            .to_vec();
        let parsed = serde_json::from_slice::<T>(&raw_body)
            .map_err(|source| PenguinClientError::ParseResponseBody { source })?;

        Ok((parsed, content_type, raw_body))
    }

    fn fetch_last_modified_once(&self, url: &str) -> Result<Option<String>, PenguinClientError> {
        let response = self
            .http_client
            .head(url)
            .send()
            .map_err(|source| PenguinClientError::SendRequest { source })?
            .error_for_status()
            .map_err(|source| PenguinClientError::HttpStatus { source })?;

        Ok(response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned))
    }
}

fn retry_delay_ms(attempt: usize) -> u64 {
    PENGUIN_FETCH_RETRY_DELAYS_MS
        .get(attempt)
        .copied()
        .or_else(|| PENGUIN_FETCH_RETRY_DELAYS_MS.last().copied())
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinMatrixResponse {
    pub rows: Vec<PenguinMatrixRow>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinStageIndexResponse {
    pub stages: Vec<PenguinStage>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinItemIndexResponse {
    pub items: Vec<PenguinItem>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PenguinMatrixRow {
    pub stage_id: String,
    pub item_id: String,
    pub times: i64,
    pub quantity: i64,
    pub std_dev: f64,
    pub start: i64,
    pub end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PenguinStage {
    pub stage_id: String,
    pub zone_id: Option<String>,
    pub stage_type: String,
    pub code: String,
    pub ap_cost: Option<i64>,
    pub existence: serde_json::Value,
    #[serde(default)]
    pub code_i18n: Option<serde_json::Value>,
    #[serde(default)]
    pub min_clear_time: Option<i64>,
    #[serde(default)]
    pub drop_infos: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PenguinItem {
    pub item_id: String,
    pub name: String,
    pub item_type: String,
    pub rarity: Option<i64>,
    pub existence: serde_json::Value,
    #[serde(default)]
    pub name_i18n: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PenguinMatrixEnvelope {
    matrix: Vec<PenguinMatrixRow>,
}

#[derive(Debug, Error)]
pub enum PenguinClientError {
    #[error("failed to build Penguin HTTP client: {source}")]
    BuildHttpClient { source: reqwest::Error },
    #[error("failed to send request to Penguin Stats: {source}")]
    SendRequest { source: reqwest::Error },
    #[error("Penguin Stats returned an unexpected HTTP status: {source}")]
    HttpStatus { source: reqwest::Error },
    #[error("failed to read Penguin Stats response body: {source}")]
    ReadResponseBody { source: reqwest::Error },
    #[error("failed to parse Penguin Stats response body: {source}")]
    ParseResponseBody { source: serde_json::Error },
}

impl PenguinClientError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::SendRequest { .. } | Self::ReadResponseBody { .. } => true,
            Self::HttpStatus { source } => source
                .status()
                .is_some_and(|status| status.is_server_error() || status.as_u16() == 429),
            Self::BuildHttpClient { .. } | Self::ParseResponseBody { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PenguinClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn client_fetches_matrix_from_http_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"matrix":[{"stageId":"main_01-07","itemId":"30011","times":100,"quantity":31,"stdDev":0.42,"start":1744012800000,"end":null}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = PenguinClient::with_matrix_url(format!("http://{address}/matrix")).unwrap();
        let matrix = client.fetch_cn_matrix().unwrap();

        assert_eq!(matrix.rows.len(), 1);
        assert_eq!(matrix.rows[0].stage_id, "main_01-07");
        assert_eq!(matrix.rows[0].item_id, "30011");
        assert!(matrix.content_type.contains("application/json"));

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_stages_and_items_from_http_endpoints() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("GET /stages ") {
                    r#"[{"stageId":"main_01-07","zoneId":"main_1","stageType":"MAIN","code":"1-7","apCost":6,"existence":{"CN":{"exist":true}}}]"#
                } else {
                    r#"[{"itemId":"30012","name":"固源岩","itemType":"MATERIAL","rarity":1,"existence":{"CN":{"exist":true}}}]"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let client = PenguinClient::with_urls(
            format!("http://{address}/matrix"),
            format!("http://{address}/stages"),
            format!("http://{address}/items"),
        )
        .unwrap();
        let stages = client.fetch_cn_stages().unwrap();
        let items = client.fetch_cn_items().unwrap();

        assert_eq!(stages.stages.len(), 1);
        assert_eq!(stages.stages[0].code, "1-7");
        assert_eq!(stages.stages[0].ap_cost, Some(6));
        assert_eq!(items.items.len(), 1);
        assert_eq!(items.items[0].name, "固源岩");
        assert_eq!(items.items[0].item_type, "MATERIAL");

        server.join().unwrap();
    }

    #[test]
    fn client_retries_after_transient_server_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_for_server = Arc::clone(&request_count);

        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 1024];
                let _ = stream.read(&mut request_buffer).unwrap();
                let attempt = request_count_for_server.fetch_add(1, Ordering::SeqCst);

                if attempt == 0 {
                    let body = r#"{"error":"temporary"}"#;
                    let response = format!(
                        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                } else {
                    let body = r#"{"matrix":[{"stageId":"main_01-07","itemId":"30011","times":100,"quantity":31,"stdDev":0.42,"start":1744012800000,"end":null}]}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                }
            }
        });

        let client = PenguinClient::with_matrix_url(format!("http://{address}/matrix")).unwrap();
        let matrix = client.fetch_cn_matrix().unwrap();

        assert_eq!(matrix.rows.len(), 1);
        assert_eq!(request_count.load(Ordering::SeqCst), 2);

        server.join().unwrap();
    }
}
