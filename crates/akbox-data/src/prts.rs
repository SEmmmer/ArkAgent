use reqwest::blocking::Client;
use serde::Deserialize;
use thiserror::Error;

pub const DEFAULT_PRTS_API_URL: &str =
    "https://prts.wiki/api.php?action=query&meta=siteinfo&siprop=general&format=json";

#[derive(Debug, Clone)]
pub struct PrtsClient {
    http_client: Client,
    api_url: String,
}

impl PrtsClient {
    pub fn new() -> Result<Self, PrtsClientError> {
        Self::with_api_url(DEFAULT_PRTS_API_URL)
    }

    pub fn with_api_url(api_url: impl Into<String>) -> Result<Self, PrtsClientError> {
        let http_client = Client::builder()
            .user_agent("ArkAgent/0.1 (https://github.com/openai/codex)")
            .build()
            .map_err(|source| PrtsClientError::BuildHttpClient { source })?;

        Ok(Self {
            http_client,
            api_url: api_url.into(),
        })
    }

    pub fn fetch_site_info(&self) -> Result<PrtsSiteInfoResponse, PrtsClientError> {
        let response = self
            .http_client
            .get(&self.api_url)
            .send()
            .map_err(|source| PrtsClientError::SendRequest { source })?
            .error_for_status()
            .map_err(|source| PrtsClientError::HttpStatus { source })?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let raw_body = response
            .bytes()
            .map_err(|source| PrtsClientError::ReadResponseBody { source })?
            .to_vec();
        let parsed = serde_json::from_slice::<PrtsSiteInfoEnvelope>(&raw_body)
            .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
        let general = parsed.query.general;

        Ok(PrtsSiteInfoResponse {
            sitename: general.sitename,
            generator: general.generator,
            base: general.base,
            server: general.server,
            server_time: general.time,
            content_type,
            raw_body,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrtsSiteInfoResponse {
    pub sitename: String,
    pub generator: String,
    pub base: String,
    pub server: String,
    pub server_time: String,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct PrtsSiteInfoEnvelope {
    query: PrtsQuery,
}

#[derive(Debug, Deserialize)]
struct PrtsQuery {
    general: PrtsGeneral,
}

#[derive(Debug, Deserialize)]
struct PrtsGeneral {
    sitename: String,
    generator: String,
    base: String,
    server: String,
    time: String,
}

#[derive(Debug, Error)]
pub enum PrtsClientError {
    #[error("failed to build PRTS HTTP client: {source}")]
    BuildHttpClient { source: reqwest::Error },
    #[error("failed to send request to PRTS: {source}")]
    SendRequest { source: reqwest::Error },
    #[error("PRTS returned an unexpected HTTP status: {source}")]
    HttpStatus { source: reqwest::Error },
    #[error("failed to read PRTS response body: {source}")]
    ReadResponseBody { source: reqwest::Error },
    #[error("failed to parse PRTS response body: {source}")]
    ParseResponseBody { source: serde_json::Error },
}

#[cfg(test)]
mod tests {
    use super::PrtsClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn client_fetches_site_info_from_http_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"query":{"general":{"sitename":"PRTS","generator":"MediaWiki 1.43.5","base":"https://prts.wiki/w/首页","server":"//prts.wiki","time":"2026-03-16T01:00:00Z"}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = PrtsClient::with_api_url(format!("http://{address}/api.php")).unwrap();
        let site_info = client.fetch_site_info().unwrap();

        assert_eq!(site_info.sitename, "PRTS");
        assert_eq!(site_info.generator, "MediaWiki 1.43.5");
        assert_eq!(site_info.server_time, "2026-03-16T01:00:00Z");
        assert!(site_info.content_type.contains("application/json"));

        server.join().unwrap();
    }
}
