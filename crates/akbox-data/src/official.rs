use std::collections::HashSet;
use std::str;

use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use time::Date;
use time::Month;
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::Time;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

pub const DEFAULT_OFFICIAL_NEWS_URL: &str = "https://ak.hypergryph.com/news";

#[derive(Debug, Clone)]
pub struct OfficialNoticeClient {
    http_client: Client,
    news_url: String,
}

impl OfficialNoticeClient {
    pub fn new() -> Result<Self, OfficialNoticeClientError> {
        Self::with_news_url(DEFAULT_OFFICIAL_NEWS_URL)
    }

    pub fn with_news_url(news_url: impl Into<String>) -> Result<Self, OfficialNoticeClientError> {
        let http_client = Client::builder()
            .user_agent("ArkAgent/0.1 (https://github.com/openai/codex)")
            .build()
            .map_err(|source| OfficialNoticeClientError::BuildHttpClient { source })?;

        Ok(Self {
            http_client,
            news_url: news_url.into(),
        })
    }

    pub fn fetch_notice_index(
        &self,
    ) -> Result<OfficialNoticeIndexResponse, OfficialNoticeClientError> {
        let response = self
            .http_client
            .get(&self.news_url)
            .send()
            .map_err(|source| OfficialNoticeClientError::SendRequest { source })?
            .error_for_status()
            .map_err(|source| OfficialNoticeClientError::HttpStatus { source })?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let raw_body = response
            .bytes()
            .map_err(|source| OfficialNoticeClientError::ReadResponseBody { source })?
            .to_vec();
        let html = str::from_utf8(&raw_body)
            .map_err(|source| OfficialNoticeClientError::Utf8ResponseBody { source })?;
        let notices = extract_notice_entries(html)?;

        Ok(OfficialNoticeIndexResponse {
            notices,
            content_type,
            raw_body,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfficialNoticeIndexResponse {
    pub notices: Vec<OfficialNoticeFeedEntry>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfficialNoticeFeedEntry {
    pub notice_id: String,
    pub title: String,
    pub notice_type: String,
    pub author: String,
    pub published_at: String,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub source_url: String,
    pub brief: String,
    pub raw_json: serde_json::Value,
}

fn extract_notice_entries(
    html: &str,
) -> Result<Vec<OfficialNoticeFeedEntry>, OfficialNoticeClientError> {
    let payloads = extract_flight_payloads(html)?;
    let initial_data = payloads
        .iter()
        .find_map(|payload| parse_initial_data_payload(payload).transpose())
        .transpose()?
        .ok_or(OfficialNoticeClientError::MissingInitialDataPayload)?;

    let mut notices = Vec::new();
    let mut seen_notice_ids = HashSet::new();

    append_section_entries(
        &mut notices,
        &mut seen_notice_ids,
        "notice",
        &initial_data.notice.list,
    )?;
    append_section_entries(
        &mut notices,
        &mut seen_notice_ids,
        "activity",
        &initial_data.activity.list,
    )?;
    append_section_entries(
        &mut notices,
        &mut seen_notice_ids,
        "news",
        &initial_data.news.list,
    )?;

    Ok(notices)
}

fn append_section_entries(
    notices: &mut Vec<OfficialNoticeFeedEntry>,
    seen_notice_ids: &mut HashSet<String>,
    notice_type: &str,
    items: &[OfficialNoticeItem],
) -> Result<(), OfficialNoticeClientError> {
    for item in items {
        if !seen_notice_ids.insert(item.cid.clone()) {
            continue;
        }

        let published_at = format_unix_timestamp_shanghai(item.display_time)?;
        let published_at_local = shanghai_datetime_from_unix(item.display_time)?;
        let (start_at, end_at) = extract_notice_window(&item.brief, published_at_local);

        notices.push(OfficialNoticeFeedEntry {
            notice_id: item.cid.clone(),
            title: item.title.clone(),
            notice_type: notice_type.to_string(),
            author: item.author.clone(),
            published_at,
            start_at,
            end_at,
            source_url: official_notice_url(&item.cid),
            brief: item.brief.clone(),
            raw_json: json!({
                "notice_type": notice_type,
                "notice": item,
            }),
        });
    }

    Ok(())
}

fn extract_flight_payloads(html: &str) -> Result<Vec<String>, OfficialNoticeClientError> {
    let marker = "self.__next_f.push([";
    let mut payloads = Vec::new();
    let mut cursor = 0;

    while let Some(relative_start) = html[cursor..].find(marker) {
        let mut position = cursor + relative_start + marker.len();

        while html
            .as_bytes()
            .get(position)
            .is_some_and(u8::is_ascii_digit)
        {
            position += 1;
        }

        if html.as_bytes().get(position) != Some(&b',') {
            cursor = position.saturating_add(1);
            continue;
        }

        position += 1;
        while html
            .as_bytes()
            .get(position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            position += 1;
        }

        if html.as_bytes().get(position) != Some(&b'"') {
            cursor = position.saturating_add(1);
            continue;
        }

        let string_end = find_json_string_end(html, position)
            .ok_or(OfficialNoticeClientError::MalformedFlightPayload)?;
        let decoded = serde_json::from_str::<String>(&html[position..=string_end])
            .map_err(|source| OfficialNoticeClientError::DecodeFlightPayload { source })?;
        payloads.push(decoded);
        cursor = string_end.saturating_add(1);
    }

    if payloads.is_empty() {
        return Err(OfficialNoticeClientError::MissingFlightPayload);
    }

    Ok(payloads)
}

fn find_json_string_end(source: &str, quote_index: usize) -> Option<usize> {
    let mut escaped = false;

    for (offset, byte) in source.as_bytes()[quote_index + 1..].iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        match *byte {
            b'\\' => escaped = true,
            b'"' => return Some(quote_index + 1 + offset),
            _ => {}
        }
    }

    None
}

fn parse_initial_data_payload(
    payload: &str,
) -> Result<Option<OfficialInitialData>, OfficialNoticeClientError> {
    if !payload.starts_with("d:") || !payload.contains("\"initialData\"") {
        return Ok(None);
    }

    let array = serde_json::from_str::<Vec<serde_json::Value>>(&payload[2..])
        .map_err(|source| OfficialNoticeClientError::ParseFlightPayload { source })?;
    let Some(initial_data) = array.get(3).and_then(|value| value.get("initialData")) else {
        return Ok(None);
    };

    serde_json::from_value::<OfficialInitialData>(initial_data.clone())
        .map(Some)
        .map_err(|source| OfficialNoticeClientError::ParseInitialData { source })
}

fn official_notice_url(notice_id: &str) -> String {
    format!("https://ak.hypergryph.com/news/{notice_id}")
}

fn shanghai_offset() -> UtcOffset {
    UtcOffset::from_hms(8, 0, 0).expect("Asia/Shanghai offset should always be valid")
}

fn shanghai_datetime_from_unix(seconds: i64) -> Result<OffsetDateTime, OfficialNoticeClientError> {
    OffsetDateTime::from_unix_timestamp(seconds)
        .map(|value| value.to_offset(shanghai_offset()))
        .map_err(|source| OfficialNoticeClientError::InvalidTimestamp { source })
}

fn format_unix_timestamp_shanghai(seconds: i64) -> Result<String, OfficialNoticeClientError> {
    shanghai_datetime_from_unix(seconds)?
        .format(&Rfc3339)
        .map_err(|source| OfficialNoticeClientError::FormatTimestamp { source })
}

fn extract_notice_window(
    brief: &str,
    published_at: OffsetDateTime,
) -> (Option<String>, Option<String>) {
    let Some(segment) = extract_time_segment(brief) else {
        return (None, None);
    };

    let Some(start_match) = find_first_date_time(segment, published_at.year()) else {
        return (None, None);
    };
    let Some(start_at) = build_shanghai_datetime(
        start_match.year,
        start_match.month,
        start_match.day,
        start_match.hour,
        start_match.minute,
    ) else {
        return (None, None);
    };

    let Some(after_separator) = slice_after_range_separator(&segment[start_match.end_index..])
    else {
        return (format_datetime_optional(start_at), None);
    };
    let end_at = find_first_date_time(after_separator, start_match.year)
        .and_then(|value| {
            build_shanghai_datetime(value.year, value.month, value.day, value.hour, value.minute)
        })
        .or_else(|| {
            find_first_time_only(after_separator).and_then(|value| {
                build_shanghai_datetime(
                    start_match.year,
                    start_match.month,
                    start_match.day,
                    value.hour,
                    value.minute,
                )
            })
        });

    (
        format_datetime_optional(start_at),
        end_at.and_then(format_datetime_optional),
    )
}

fn extract_time_segment(brief: &str) -> Option<&str> {
    const LABELS: [&str; 6] = [
        "关卡开放时间：",
        "活动开启时间：",
        "活动时间：",
        "限时开启活动时间：",
        "开启时间：",
        "开放时间：",
    ];

    for label in LABELS {
        if let Some((_, tail)) = brief.split_once(label) {
            return Some(tail);
        }
    }

    if brief.contains("停机维护") || brief.contains("闪断更新") {
        return Some(brief);
    }

    None
}

fn slice_after_range_separator(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();

    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'-' && *byte != b'~' {
            continue;
        }

        let mut tail = index + 1;
        while bytes.get(tail).is_some_and(u8::is_ascii_whitespace) {
            tail += 1;
        }

        return Some(&text[tail..]);
    }

    None
}

fn find_first_date_time(text: &str, default_year: i32) -> Option<DateTimeMatch> {
    let indices = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut starts = vec![0];
    starts.extend(indices.into_iter().skip(1));

    for start in starts {
        if let Some(mut value) = parse_year_month_day_time(&text[start..]) {
            value.end_index += start;
            return Some(value);
        }
        if let Some(mut value) = parse_month_day_time(&text[start..], default_year) {
            value.end_index += start;
            return Some(value);
        }
    }

    None
}

fn find_first_time_only(text: &str) -> Option<TimeMatch> {
    let indices = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut starts = vec![0];
    starts.extend(indices.into_iter().skip(1));

    for start in starts {
        if let Some(value) = parse_time_only(&text[start..]) {
            return Some(value);
        }
    }

    None
}

fn parse_year_month_day_time(text: &str) -> Option<DateTimeMatch> {
    let (year, year_end) = parse_digits(text, 0, 4, 4)?;
    let month_start = consume_expected_char(text, year_end, '年')?;
    let (month, month_end) = parse_digits(text, month_start, 1, 2)?;
    let day_start = consume_expected_char(text, month_end, '月')?;
    let (day, day_end) = parse_digits(text, day_start, 1, 2)?;
    let time_start = consume_date_time_separator(text, day_end, '日')?;
    let (hour, minute, end_index) = parse_hour_minute(text, time_start)?;

    Some(DateTimeMatch {
        year: year as i32,
        month: month as u8,
        day: day as u8,
        hour: hour as u8,
        minute: minute as u8,
        end_index,
    })
}

fn parse_month_day_time(text: &str, default_year: i32) -> Option<DateTimeMatch> {
    let (month, month_end) = parse_digits(text, 0, 1, 2)?;
    let day_start = consume_expected_char(text, month_end, '月')?;
    let (day, day_end) = parse_digits(text, day_start, 1, 2)?;
    let time_start = consume_date_time_separator(text, day_end, '日')?;
    let (hour, minute, end_index) = parse_hour_minute(text, time_start)?;

    Some(DateTimeMatch {
        year: default_year,
        month: month as u8,
        day: day as u8,
        hour: hour as u8,
        minute: minute as u8,
        end_index,
    })
}

fn parse_time_only(text: &str) -> Option<TimeMatch> {
    let (hour, hour_end) = parse_digits(text, 0, 1, 2)?;
    let minute_start = consume_expected_char(text, hour_end, ':')?;
    let (minute, _) = parse_digits(text, minute_start, 2, 2)?;

    Some(TimeMatch {
        hour: hour as u8,
        minute: minute as u8,
    })
}

fn parse_hour_minute(text: &str, start: usize) -> Option<(u16, u16, usize)> {
    let (hour, hour_end) = parse_digits(text, start, 1, 2)?;
    let minute_start = consume_expected_char(text, hour_end, ':')?;
    let (minute, minute_end) = parse_digits(text, minute_start, 2, 2)?;
    Some((hour, minute, minute_end))
}

fn parse_digits(text: &str, start: usize, min: usize, max: usize) -> Option<(u16, usize)> {
    let bytes = text.as_bytes();
    let mut end = start;

    while end < bytes.len() && bytes[end].is_ascii_digit() && end - start < max {
        end += 1;
    }

    if end - start < min {
        return None;
    }

    text[start..end]
        .parse::<u16>()
        .ok()
        .map(|value| (value, end))
}

fn consume_expected_char(text: &str, start: usize, expected: char) -> Option<usize> {
    let slice = text.get(start..)?;
    let mut chars = slice.chars();
    let character = chars.next()?;
    if character != expected {
        return None;
    }

    Some(start + character.len_utf8())
}

fn consume_date_time_separator(text: &str, start: usize, expected: char) -> Option<usize> {
    let mut next_index = consume_expected_char(text, start, expected)?;

    while text
        .as_bytes()
        .get(next_index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        next_index += 1;
    }

    Some(next_index)
}

fn build_shanghai_datetime(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
) -> Option<OffsetDateTime> {
    let month = Month::try_from(month).ok()?;
    let date = Date::from_calendar_date(year, month, day).ok()?;
    let time = Time::from_hms(hour, minute, 0).ok()?;

    Some(PrimitiveDateTime::new(date, time).assume_offset(shanghai_offset()))
}

fn format_datetime_optional(value: OffsetDateTime) -> Option<String> {
    value.format(&Rfc3339).ok()
}

#[derive(Debug, Clone, Copy)]
struct DateTimeMatch {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    end_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct TimeMatch {
    hour: u8,
    minute: u8,
}

#[derive(Debug, Deserialize)]
struct OfficialInitialData {
    #[serde(rename = "NOTICE", default)]
    notice: OfficialNoticeSection,
    #[serde(rename = "ACTIVITY", default)]
    activity: OfficialNoticeSection,
    #[serde(rename = "NEWS", default)]
    news: OfficialNoticeSection,
}

#[derive(Debug, Default, Deserialize)]
struct OfficialNoticeSection {
    #[serde(default)]
    list: Vec<OfficialNoticeItem>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OfficialNoticeItem {
    cid: String,
    tab: String,
    #[serde(default)]
    sticky: bool,
    title: String,
    author: String,
    #[serde(rename = "displayTime")]
    display_time: i64,
    #[serde(default)]
    cover: String,
    #[serde(rename = "extraCover", default)]
    extra_cover: String,
    brief: String,
}

#[derive(Debug, Error)]
pub enum OfficialNoticeClientError {
    #[error("failed to build official notice HTTP client: {source}")]
    BuildHttpClient { source: reqwest::Error },
    #[error("failed to send request to official notice source: {source}")]
    SendRequest { source: reqwest::Error },
    #[error("official notice source returned an unexpected HTTP status: {source}")]
    HttpStatus { source: reqwest::Error },
    #[error("failed to read official notice response body: {source}")]
    ReadResponseBody { source: reqwest::Error },
    #[error("official notice response body is not valid UTF-8: {source}")]
    Utf8ResponseBody { source: str::Utf8Error },
    #[error("official notice page does not contain any flight payloads")]
    MissingFlightPayload,
    #[error("official notice page contains a malformed flight payload")]
    MalformedFlightPayload,
    #[error("failed to decode official notice flight payload: {source}")]
    DecodeFlightPayload { source: serde_json::Error },
    #[error("failed to parse official notice flight payload: {source}")]
    ParseFlightPayload { source: serde_json::Error },
    #[error("failed to parse official notice initial data: {source}")]
    ParseInitialData { source: serde_json::Error },
    #[error("official notice page does not contain initial data")]
    MissingInitialDataPayload,
    #[error("official notice payload contains an invalid timestamp: {source}")]
    InvalidTimestamp { source: time::error::ComponentRange },
    #[error("failed to format official notice timestamp: {source}")]
    FormatTimestamp { source: time::error::Format },
}

#[cfg(test)]
mod tests {
    use super::OfficialNoticeClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn client_fetches_notices_from_html_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 2048];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = test_html();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = OfficialNoticeClient::with_news_url(format!("http://{address}/news")).unwrap();
        let response = client.fetch_notice_index().unwrap();

        assert_eq!(response.notices.len(), 3);
        assert_eq!(response.notices[0].notice_type, "notice");
        assert_eq!(response.notices[1].notice_type, "activity");
        assert_eq!(response.notices[2].notice_type, "news");
        assert_eq!(
            response.notices[1].start_at.as_deref(),
            Some("2026-03-14T16:00:00+08:00")
        );
        assert_eq!(
            response.notices[1].end_at.as_deref(),
            Some("2026-04-25T03:59:00+08:00")
        );
        assert!(response.content_type.contains("text/html"));

        server.join().unwrap();
    }

    #[test]
    fn client_parses_maintenance_window_from_notice_brief() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 2048];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = maintenance_only_html();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = OfficialNoticeClient::with_news_url(format!("http://{address}/news")).unwrap();
        let response = client.fetch_notice_index().unwrap();

        assert_eq!(
            response.notices[0].start_at.as_deref(),
            Some("2026-01-24T16:00:00+08:00")
        );
        assert_eq!(
            response.notices[0].end_at.as_deref(),
            Some("2026-01-24T16:10:00+08:00")
        );

        server.join().unwrap();
    }

    fn test_html() -> String {
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
                            "cid": "9697",
                            "tab": "1",
                            "sticky": false,
                            "title": "[活动预告] 「卫戍协议：盟约」限时活动即将开启",
                            "author": "【明日方舟】运营组",
                            "displayTime": 1773198000_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "一、「卫戍协议：盟约」限时活动开启关卡开放时间：03月14日 16:00 - 04月25日 03:59解锁条件：通关主线1-10"
                        }]
                    },
                    "NEWS": {
                        "list": [{
                            "cid": "9698",
                            "tab": "2",
                            "sticky": false,
                            "title": "《明日方舟》制作组通讯#62期",
                            "author": "【明日方舟】制作组",
                            "displayTime": 1771923600_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "感谢大家一直以来对《明日方舟》的关注与支持。"
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

    fn maintenance_only_html() -> String {
        let payload = serde_json::json!([
            "$",
            "$L18",
            serde_json::Value::Null,
            {
                "initialData": {
                    "NOTICE": {
                        "list": [{
                            "cid": "2834",
                            "tab": "0",
                            "sticky": false,
                            "title": "[明日方舟]01月24日16:00闪断更新公告",
                            "author": "【明日方舟】运营组",
                            "displayTime": 1769223600_i64,
                            "cover": "",
                            "extraCover": "",
                            "brief": "感谢您对《明日方舟》的关注与支持。《明日方舟》计划将于2026年01月24日16:00 ~ 16:10期间进行服务器闪断更新。"
                        }]
                    },
                    "ACTIVITY": {
                        "list": []
                    },
                    "NEWS": {
                        "list": []
                    }
                }
            }
        ]);

        let encoded = serde_json::to_string(&format!("d:{payload}")).unwrap();
        format!(
            "<html><body><script>self.__next_f = self.__next_f || []; self.__next_f.push([1,{encoded}])</script></body></html>"
        )
    }
}
