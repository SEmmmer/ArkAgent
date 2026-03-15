use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

pub const DEFAULT_PENGUIN_MATRIX_URL: &str =
    "https://penguin-stats.io/PenguinStats/api/v2/result/matrix?server=CN";

#[derive(Debug, Clone)]
pub struct PenguinClient {
    http_client: Client,
    matrix_url: String,
}

impl PenguinClient {
    pub fn new() -> Result<Self, PenguinClientError> {
        Self::with_matrix_url(DEFAULT_PENGUIN_MATRIX_URL)
    }

    pub fn with_matrix_url(matrix_url: impl Into<String>) -> Result<Self, PenguinClientError> {
        let http_client = Client::builder()
            .user_agent("ArkAgent/0.1 (https://github.com/openai/codex)")
            .build()
            .map_err(|source| PenguinClientError::BuildHttpClient { source })?;

        Ok(Self {
            http_client,
            matrix_url: matrix_url.into(),
        })
    }

    pub fn fetch_cn_matrix(&self) -> Result<PenguinMatrixResponse, PenguinClientError> {
        let response = self
            .http_client
            .get(&self.matrix_url)
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
        let parsed = serde_json::from_slice::<PenguinMatrixEnvelope>(&raw_body)
            .map_err(|source| PenguinClientError::ParseResponseBody { source })?;

        Ok(PenguinMatrixResponse {
            rows: parsed.matrix,
            content_type,
            raw_body,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenguinMatrixResponse {
    pub rows: Vec<PenguinMatrixRow>,
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

#[cfg(test)]
mod tests {
    use super::PenguinClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
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
}
