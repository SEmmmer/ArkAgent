use std::collections::BTreeMap;
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use scraper::ElementRef;
use scraper::Html;
use scraper::Selector;
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;

pub const DEFAULT_PRTS_API_URL: &str =
    "https://prts.wiki/api.php?action=query&meta=siteinfo&siprop=general&format=json";
pub const DEFAULT_PRTS_ITEM_INDEX_URL: &str = "https://prts.wiki/api.php?action=parse&page=%E9%81%93%E5%85%B7%E4%B8%80%E8%A7%88&prop=revid%7Ctext&format=json";
pub const DEFAULT_PRTS_RECIPE_INDEX_URL: &str = "https://prts.wiki/api.php?action=parse&page=%E7%BD%97%E5%BE%B7%E5%B2%9B%E5%9F%BA%E5%BB%BA/%E5%8A%A0%E5%B7%A5%E7%AB%99&prop=revid%7Ctext&format=json";

const PRTS_OPERATOR_ELITE_GROWTH_SECTION: &str = "精英化材料";
const PRTS_OPERATOR_SKILL_GROWTH_SECTION: &str = "技能升级材料";
const PRTS_OPERATOR_BUILDING_SKILL_SECTION: &str = "后勤技能";
const PRTS_OPERATOR_INDEX_PAGE_TITLE: &str = "干员一览";
const PRTS_OPERATOR_INDEX_QUERY_LIMIT: usize = 500;
const PRTS_STAGE_INDEX_PAGE_TITLE: &str = "关卡一览";
const PRTS_STAGE_INDEX_QUERY_LIMIT: usize = 500;
const PRTS_FETCH_MAX_ATTEMPTS: usize = 3;
const PRTS_FETCH_RETRY_DELAYS_MS: [u64; 2] = [500, 1_500];

#[derive(Debug, Clone)]
pub struct PrtsClient {
    http_client: Client,
    site_info_url: String,
    item_index_url: String,
    recipe_index_url: String,
}

impl PrtsClient {
    pub fn new() -> Result<Self, PrtsClientError> {
        Self::with_urls_and_recipe(
            DEFAULT_PRTS_API_URL,
            DEFAULT_PRTS_ITEM_INDEX_URL,
            DEFAULT_PRTS_RECIPE_INDEX_URL,
        )
    }

    pub fn with_api_url(api_url: impl Into<String>) -> Result<Self, PrtsClientError> {
        Self::with_urls_and_recipe(
            api_url,
            DEFAULT_PRTS_ITEM_INDEX_URL,
            DEFAULT_PRTS_RECIPE_INDEX_URL,
        )
    }

    pub fn with_urls(
        site_info_url: impl Into<String>,
        item_index_url: impl Into<String>,
    ) -> Result<Self, PrtsClientError> {
        Self::with_urls_and_recipe(site_info_url, item_index_url, DEFAULT_PRTS_RECIPE_INDEX_URL)
    }

    pub fn with_urls_and_recipe(
        site_info_url: impl Into<String>,
        item_index_url: impl Into<String>,
        recipe_index_url: impl Into<String>,
    ) -> Result<Self, PrtsClientError> {
        let http_client = Client::builder()
            .user_agent("ArkAgent/0.1 (https://github.com/openai/codex)")
            .build()
            .map_err(|source| PrtsClientError::BuildHttpClient { source })?;

        Ok(Self {
            http_client,
            site_info_url: site_info_url.into(),
            item_index_url: item_index_url.into(),
            recipe_index_url: recipe_index_url.into(),
        })
    }

    pub fn fetch_site_info(&self) -> Result<PrtsSiteInfoResponse, PrtsClientError> {
        let response = self
            .http_client
            .get(&self.site_info_url)
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

    pub fn fetch_item_index(&self) -> Result<PrtsItemIndexResponse, PrtsClientError> {
        let (content_type, raw_body) = self.fetch_raw_body(&self.item_index_url)?;
        let parsed = serde_json::from_slice::<PrtsItemIndexEnvelope>(&raw_body)
            .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
        let items = extract_item_definitions(&parsed.parse.text.html)?;

        Ok(PrtsItemIndexResponse {
            revision: parsed.parse.revid.to_string(),
            items,
            content_type,
            raw_body,
        })
    }

    pub fn fetch_item_index_revision(&self) -> Result<String, PrtsClientError> {
        self.fetch_parse_page_revision("道具一览")
    }

    pub fn fetch_operator_index(&self) -> Result<PrtsOperatorIndexResponse, PrtsClientError> {
        let revision = self.fetch_page_revision(PRTS_OPERATOR_INDEX_PAGE_TITLE)?;
        let mut content_type = None;
        let mut raw_pages = Vec::new();
        let mut operators = Vec::new();
        let mut offset = None;

        loop {
            let query = build_operator_index_query(offset);
            let request_url = self.build_api_url(&[
                ("action", "ask"),
                ("query", query.as_str()),
                ("format", "json"),
            ])?;
            let (page_content_type, raw_body) = self.fetch_raw_body(&request_url)?;
            if content_type.is_none() {
                content_type = Some(page_content_type);
            }

            let parsed = serde_json::from_slice::<PrtsOperatorIndexEnvelope>(&raw_body)
                .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
            let raw_page = serde_json::from_slice::<serde_json::Value>(&raw_body)
                .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
            operators.extend(extract_operator_definitions(&parsed)?);
            raw_pages.push(raw_page);

            if let Some(next_offset) = parsed.query_continue_offset {
                offset = Some(next_offset);
            } else {
                break;
            }
        }

        let raw_body = serde_json::to_vec(&json!({
            "page_title": PRTS_OPERATOR_INDEX_PAGE_TITLE,
            "revision": revision,
            "pages": raw_pages,
        }))
        .map_err(|source| PrtsClientError::SerializeResponseBody { source })?;

        Ok(PrtsOperatorIndexResponse {
            revision,
            operators,
            content_type: content_type.unwrap_or_else(|| "application/json".to_string()),
            raw_body,
        })
    }

    pub fn fetch_operator_index_revision(&self) -> Result<String, PrtsClientError> {
        self.fetch_page_revision(PRTS_OPERATOR_INDEX_PAGE_TITLE)
    }

    pub fn fetch_stage_index(&self) -> Result<PrtsStageIndexResponse, PrtsClientError> {
        let revision = self.fetch_page_revision(PRTS_STAGE_INDEX_PAGE_TITLE)?;
        let mut content_type = None;
        let mut raw_pages = Vec::new();
        let mut stages = Vec::new();
        let mut offset = None;

        loop {
            let query = build_stage_index_query(offset);
            let request_url = self.build_api_url(&[
                ("action", "ask"),
                ("query", query.as_str()),
                ("format", "json"),
            ])?;
            let (page_content_type, raw_body) = self.fetch_raw_body(&request_url)?;
            if content_type.is_none() {
                content_type = Some(page_content_type);
            }

            let parsed = serde_json::from_slice::<PrtsStageIndexEnvelope>(&raw_body)
                .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
            let raw_page = serde_json::from_slice::<serde_json::Value>(&raw_body)
                .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
            stages.extend(extract_stage_definitions(&parsed)?);
            raw_pages.push(raw_page);

            if let Some(next_offset) = parsed.query_continue_offset {
                offset = Some(next_offset);
            } else {
                break;
            }
        }

        let raw_body = serde_json::to_vec(&json!({
            "page_title": PRTS_STAGE_INDEX_PAGE_TITLE,
            "revision": revision,
            "pages": raw_pages,
        }))
        .map_err(|source| PrtsClientError::SerializeResponseBody { source })?;

        Ok(PrtsStageIndexResponse {
            revision,
            stages,
            content_type: content_type.unwrap_or_else(|| "application/json".to_string()),
            raw_body,
        })
    }

    pub fn fetch_stage_index_revision(&self) -> Result<String, PrtsClientError> {
        self.fetch_page_revision(PRTS_STAGE_INDEX_PAGE_TITLE)
    }

    pub fn fetch_recipe_index(&self) -> Result<PrtsRecipeIndexResponse, PrtsClientError> {
        let (content_type, raw_body) = self.fetch_raw_body(&self.recipe_index_url)?;
        let parsed = serde_json::from_slice::<PrtsItemIndexEnvelope>(&raw_body)
            .map_err(|source| PrtsClientError::ParseResponseBody { source })?;
        let recipes = extract_recipe_definitions(&parsed.parse.text.html)?;

        Ok(PrtsRecipeIndexResponse {
            revision: parsed.parse.revid.to_string(),
            recipes,
            content_type,
            raw_body,
        })
    }

    pub fn fetch_recipe_index_revision(&self) -> Result<String, PrtsClientError> {
        self.fetch_parse_page_revision("罗德岛基建/加工站")
    }

    pub fn fetch_operator_growth(&self) -> Result<PrtsOperatorGrowthResponse, PrtsClientError> {
        let operator_index = self.fetch_operator_index()?;
        let collectible_operators = operator_index
            .operators
            .iter()
            .filter(|operator| operator.is_box_collectible)
            .cloned()
            .collect::<Vec<_>>();
        let mut latest_revision = operator_index.revision.parse::<i64>().unwrap_or_default();
        let mut content_type = Some(operator_index.content_type.clone());
        let mut raw_pages = Vec::new();
        let mut growths = Vec::new();

        for operator in &collectible_operators {
            let sections = self.fetch_page_sections(&operator.page_title)?;
            latest_revision =
                latest_revision.max(sections.revision.parse::<i64>().unwrap_or_default());
            if content_type.is_none() {
                content_type = Some(sections.content_type.clone());
            }

            let mut raw_sections = Vec::new();
            for section_name in [
                PRTS_OPERATOR_ELITE_GROWTH_SECTION,
                PRTS_OPERATOR_SKILL_GROWTH_SECTION,
            ] {
                let Some(section_index) = sections
                    .sections
                    .iter()
                    .find(|section| section.line == section_name)
                    .map(|section| section.index.as_str())
                else {
                    continue;
                };

                let (section_content_type, section_html) =
                    self.fetch_page_section_html(&operator.page_title, section_index)?;
                if content_type.is_none() {
                    content_type = Some(section_content_type);
                }

                growths.extend(extract_operator_growth_definitions(
                    operator,
                    section_name,
                    &section_html,
                )?);
                raw_sections.push(json!({
                    "line": section_name,
                    "index": section_index,
                    "html": section_html,
                }));
            }

            raw_pages.push(json!({
                "operator_id": operator.operator_id,
                "page_title": operator.page_title,
                "page_revision": sections.revision,
                "sections": raw_sections,
            }));
        }

        let raw_body = serde_json::to_vec(&json!({
            "operator_index_revision": operator_index.revision,
            "pages": raw_pages,
        }))
        .map_err(|source| PrtsClientError::SerializeResponseBody { source })?;

        Ok(PrtsOperatorGrowthResponse {
            revision: if latest_revision > 0 {
                latest_revision.to_string()
            } else {
                operator_index.revision
            },
            operators: collectible_operators,
            growths,
            content_type: content_type.unwrap_or_else(|| "application/json".to_string()),
            raw_body,
        })
    }

    pub fn fetch_operator_building_skills(
        &self,
    ) -> Result<PrtsOperatorBuildingSkillResponse, PrtsClientError> {
        let operator_index = self.fetch_operator_index()?;
        let collectible_operators = operator_index
            .operators
            .iter()
            .filter(|operator| operator.is_box_collectible)
            .cloned()
            .collect::<Vec<_>>();
        let mut latest_revision = operator_index.revision.parse::<i64>().unwrap_or_default();
        let mut content_type = Some(operator_index.content_type.clone());
        let mut raw_pages = Vec::new();
        let mut building_skills = Vec::new();

        for operator in &collectible_operators {
            let sections = self.fetch_page_sections(&operator.page_title)?;
            latest_revision =
                latest_revision.max(sections.revision.parse::<i64>().unwrap_or_default());
            if content_type.is_none() {
                content_type = Some(sections.content_type.clone());
            }

            let mut raw_sections = Vec::new();
            if let Some(section_index) = sections
                .sections
                .iter()
                .find(|section| section.line == PRTS_OPERATOR_BUILDING_SKILL_SECTION)
                .map(|section| section.index.as_str())
            {
                let (section_content_type, section_html) =
                    self.fetch_page_section_html(&operator.page_title, section_index)?;
                if content_type.is_none() {
                    content_type = Some(section_content_type);
                }

                building_skills.extend(extract_operator_building_skill_definitions(
                    operator,
                    &section_html,
                )?);
                raw_sections.push(json!({
                    "line": PRTS_OPERATOR_BUILDING_SKILL_SECTION,
                    "index": section_index,
                    "html": section_html,
                }));
            }

            raw_pages.push(json!({
                "operator_id": operator.operator_id,
                "page_title": operator.page_title,
                "page_revision": sections.revision,
                "sections": raw_sections,
            }));
        }

        let raw_body = serde_json::to_vec(&json!({
            "operator_index_revision": operator_index.revision,
            "pages": raw_pages,
        }))
        .map_err(|source| PrtsClientError::SerializeResponseBody { source })?;

        Ok(PrtsOperatorBuildingSkillResponse {
            revision: if latest_revision > 0 {
                latest_revision.to_string()
            } else {
                operator_index.revision
            },
            operators: collectible_operators,
            building_skills,
            content_type: content_type.unwrap_or_else(|| "application/json".to_string()),
            raw_body,
        })
    }

    fn fetch_page_revision(&self, page_title: &str) -> Result<String, PrtsClientError> {
        let request_url = self.build_api_url(&[
            ("action", "parse"),
            ("page", page_title),
            ("prop", "revid"),
            ("format", "json"),
        ])?;
        let (_, raw_body) = self.fetch_raw_body(&request_url)?;
        let parsed = serde_json::from_slice::<PrtsPageRevisionEnvelope>(&raw_body)
            .map_err(|source| PrtsClientError::ParseResponseBody { source })?;

        Ok(parsed.parse.revid.to_string())
    }

    fn fetch_parse_page_revision(&self, page_title: &str) -> Result<String, PrtsClientError> {
        self.fetch_page_revision(page_title)
    }

    fn fetch_page_sections(
        &self,
        page_title: &str,
    ) -> Result<PrtsPageSectionsResponse, PrtsClientError> {
        let request_url = self.build_api_url(&[
            ("action", "parse"),
            ("page", page_title),
            ("prop", "sections|revid"),
            ("format", "json"),
        ])?;
        let (content_type, raw_body) = self.fetch_raw_body(&request_url)?;
        let parsed = serde_json::from_slice::<PrtsPageSectionsEnvelope>(&raw_body)
            .map_err(|source| PrtsClientError::ParseResponseBody { source })?;

        Ok(PrtsPageSectionsResponse {
            revision: parsed.parse.revid.to_string(),
            content_type,
            sections: parsed.parse.sections,
        })
    }

    fn fetch_page_section_html(
        &self,
        page_title: &str,
        section_index: &str,
    ) -> Result<(String, String), PrtsClientError> {
        let request_url = self.build_api_url(&[
            ("action", "parse"),
            ("page", page_title),
            ("prop", "text"),
            ("section", section_index),
            ("format", "json"),
        ])?;
        let (content_type, raw_body) = self.fetch_raw_body(&request_url)?;
        let parsed = serde_json::from_slice::<PrtsSectionHtmlEnvelope>(&raw_body)
            .map_err(|source| PrtsClientError::ParseResponseBody { source })?;

        Ok((content_type, parsed.parse.text.html))
    }

    fn build_api_url(&self, query_pairs: &[(&str, &str)]) -> Result<String, PrtsClientError> {
        let endpoint = self
            .site_info_url
            .split('?')
            .next()
            .unwrap_or(self.site_info_url.as_str());
        let mut url =
            reqwest::Url::parse(endpoint).map_err(|source| PrtsClientError::BuildRequestUrl {
                message: source.to_string(),
            })?;
        url.query_pairs_mut()
            .clear()
            .extend_pairs(query_pairs.iter().copied());

        Ok(url.to_string())
    }

    fn fetch_raw_body(&self, url: &str) -> Result<(String, Vec<u8>), PrtsClientError> {
        for attempt in 0..PRTS_FETCH_MAX_ATTEMPTS {
            match self.fetch_raw_body_once(url) {
                Ok(response) => return Ok(response),
                Err(error) if error.is_retryable() && attempt + 1 < PRTS_FETCH_MAX_ATTEMPTS => {
                    let delay_ms = prts_retry_delay_ms(attempt);
                    tracing::warn!(
                        url,
                        attempt = attempt + 1,
                        max_attempts = PRTS_FETCH_MAX_ATTEMPTS,
                        delay_ms,
                        error = %error,
                        "prts request failed, retrying"
                    );
                    thread::sleep(Duration::from_millis(delay_ms));
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("retry loop should have returned or errored")
    }

    fn fetch_raw_body_once(&self, url: &str) -> Result<(String, Vec<u8>), PrtsClientError> {
        let response = self
            .http_client
            .get(url)
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

        Ok((content_type, raw_body))
    }
}

fn prts_retry_delay_ms(attempt: usize) -> u64 {
    PRTS_FETCH_RETRY_DELAYS_MS
        .get(attempt)
        .copied()
        .or_else(|| PRTS_FETCH_RETRY_DELAYS_MS.last().copied())
        .unwrap_or(1_000)
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

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsItemIndexResponse {
    pub revision: String,
    pub items: Vec<PrtsItemDefinition>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsOperatorIndexResponse {
    pub revision: String,
    pub operators: Vec<PrtsOperatorDefinition>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsStageIndexResponse {
    pub revision: String,
    pub stages: Vec<PrtsStageDefinition>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsRecipeIndexResponse {
    pub revision: String,
    pub recipes: Vec<PrtsRecipeDefinition>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsOperatorGrowthResponse {
    pub revision: String,
    pub operators: Vec<PrtsOperatorDefinition>,
    pub growths: Vec<PrtsOperatorGrowthDefinition>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsOperatorBuildingSkillResponse {
    pub revision: String,
    pub operators: Vec<PrtsOperatorDefinition>,
    pub building_skills: Vec<PrtsOperatorBuildingSkillDefinition>,
    pub content_type: String,
    pub raw_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsItemDefinition {
    pub item_id: String,
    pub name_zh: String,
    pub item_type: String,
    pub rarity: Option<i64>,
    pub description: Option<String>,
    pub usage: Option<String>,
    pub obtain_approach: Option<String>,
    pub categories: Vec<String>,
    pub file_url: Option<String>,
    pub dark_background: Option<bool>,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsOperatorDefinition {
    pub operator_id: String,
    pub name_zh: String,
    pub rarity: i64,
    pub profession: String,
    pub branch: Option<String>,
    pub categories: Vec<String>,
    pub is_box_collectible: bool,
    pub page_title: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsStageDefinition {
    pub stage_id: String,
    pub zone_id: Option<String>,
    pub code: String,
    pub page_title: String,
    pub categories: Vec<String>,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsRecipeDefinition {
    pub recipe_kind: String,
    pub workshop_level: i64,
    pub output_name_zh: String,
    pub ingredients: Vec<PrtsRecipeIngredient>,
    pub lmd_cost: i64,
    pub mood_cost: i64,
    pub byproduct_rate: Option<f64>,
    pub unlock_condition: Option<String>,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsRecipeIngredient {
    pub item_name_zh: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsOperatorGrowthDefinition {
    pub operator_id: String,
    pub stage_label: String,
    pub material_slot: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrtsOperatorBuildingSkillDefinition {
    pub operator_id: String,
    pub room_type: String,
    pub skill_name: String,
    pub raw_json: serde_json::Value,
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

#[derive(Debug, Deserialize)]
struct PrtsItemIndexEnvelope {
    parse: PrtsParsedPage,
}

#[derive(Debug, Deserialize)]
struct PrtsParsedPage {
    revid: i64,
    text: PrtsParsedHtml,
}

#[derive(Debug, Deserialize)]
struct PrtsParsedHtml {
    #[serde(rename = "*")]
    html: String,
}

#[derive(Debug, Deserialize)]
struct PrtsSectionHtmlEnvelope {
    parse: PrtsParsedSectionHtml,
}

#[derive(Debug, Deserialize)]
struct PrtsParsedSectionHtml {
    text: PrtsParsedHtml,
}

#[derive(Debug, Deserialize)]
struct PrtsPageRevisionEnvelope {
    parse: PrtsParsedRevisionPage,
}

#[derive(Debug, Deserialize)]
struct PrtsParsedRevisionPage {
    revid: i64,
}

#[derive(Debug, Deserialize)]
struct PrtsPageSectionsEnvelope {
    parse: PrtsParsedPageSections,
}

#[derive(Debug, Deserialize)]
struct PrtsParsedPageSections {
    revid: i64,
    sections: Vec<PrtsPageSection>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrtsPageSection {
    line: String,
    index: String,
}

#[derive(Debug, Clone)]
struct PrtsPageSectionsResponse {
    revision: String,
    content_type: String,
    sections: Vec<PrtsPageSection>,
}

#[derive(Debug, Deserialize)]
struct PrtsStageIndexEnvelope {
    #[serde(rename = "query-continue-offset")]
    query_continue_offset: Option<usize>,
    query: PrtsStageQueryEnvelope,
}

#[derive(Debug, Deserialize)]
struct PrtsStageQueryEnvelope {
    results: BTreeMap<String, PrtsStageQueryResult>,
}

#[derive(Debug, Deserialize)]
struct PrtsOperatorIndexEnvelope {
    #[serde(rename = "query-continue-offset")]
    query_continue_offset: Option<usize>,
    query: PrtsOperatorQueryEnvelope,
}

#[derive(Debug, Deserialize)]
struct PrtsOperatorQueryEnvelope {
    results: BTreeMap<String, PrtsOperatorQueryResult>,
}

#[derive(Debug, Deserialize)]
struct PrtsOperatorQueryResult {
    printouts: PrtsOperatorPrintouts,
    fulltext: String,
    fullurl: String,
}

#[derive(Debug, Deserialize)]
struct PrtsOperatorPrintouts {
    #[serde(rename = "干员id", default)]
    operator_ids: Vec<String>,
    #[serde(rename = "稀有度", default)]
    rarities: Vec<String>,
    #[serde(rename = "职业", default)]
    professions: Vec<String>,
    #[serde(rename = "分支", default)]
    branches: Vec<PrtsPageReference>,
    #[serde(rename = "分类", default)]
    categories: Vec<PrtsPageReference>,
}

#[derive(Debug, Deserialize)]
struct PrtsStageQueryResult {
    printouts: PrtsStagePrintouts,
    fulltext: String,
    fullurl: String,
}

#[derive(Debug, Deserialize)]
struct PrtsStagePrintouts {
    #[serde(rename = "关卡id", default)]
    stage_ids: Vec<String>,
    #[serde(rename = "分类", default)]
    categories: Vec<PrtsPageReference>,
}

#[derive(Debug, Deserialize)]
struct PrtsPageReference {
    fulltext: String,
}

fn extract_item_definitions(html: &str) -> Result<Vec<PrtsItemDefinition>, PrtsClientError> {
    let marker = "<div class=\"smwdata\"";
    let mut items = Vec::new();
    let mut cursor = 0;

    while let Some(relative_start) = html[cursor..].find(marker) {
        let start = cursor + relative_start;
        let tag_end =
            html[start..]
                .find('>')
                .ok_or_else(|| PrtsClientError::ParseItemIndexHtml {
                    message: "unterminated smwdata div".to_string(),
                })?
                + start;
        let tag = &html[start..=tag_end];
        items.push(parse_item_definition(tag)?);
        cursor = tag_end + 1;
    }

    if items.is_empty() {
        return Err(PrtsClientError::ParseItemIndexHtml {
            message: "item index HTML does not contain any smwdata entries".to_string(),
        });
    }

    Ok(items)
}

fn extract_operator_definitions(
    payload: &PrtsOperatorIndexEnvelope,
) -> Result<Vec<PrtsOperatorDefinition>, PrtsClientError> {
    if payload.query.results.is_empty() {
        return Err(PrtsClientError::ParseOperatorIndexPayload {
            message: "operator index query returned no results".to_string(),
        });
    }

    payload
        .query
        .results
        .values()
        .map(|result| {
            let operator_id = result
                .printouts
                .operator_ids
                .first()
                .cloned()
                .ok_or_else(|| PrtsClientError::ParseOperatorIndexPayload {
                    message: format!("operator page {} is missing 干员id", result.fulltext),
                })?;
            let rarity = result
                .printouts
                .rarities
                .first()
                .and_then(|value| value.parse::<i64>().ok())
                .ok_or_else(|| PrtsClientError::ParseOperatorIndexPayload {
                    message: format!("operator page {} is missing 稀有度", result.fulltext),
                })?;
            let profession = result
                .printouts
                .professions
                .first()
                .cloned()
                .ok_or_else(|| PrtsClientError::ParseOperatorIndexPayload {
                    message: format!("operator page {} is missing 职业", result.fulltext),
                })?;
            let branch = result
                .printouts
                .branches
                .first()
                .map(|value| value.fulltext.clone());
            let categories = result
                .printouts
                .categories
                .iter()
                .map(|category| {
                    category
                        .fulltext
                        .strip_prefix("分类:")
                        .unwrap_or(category.fulltext.as_str())
                        .to_string()
                })
                .collect::<Vec<_>>();
            let availability_kind = operator_availability_kind(&result.fulltext, &categories);
            let is_box_collectible = availability_kind == "box_collectible";
            let page_url = normalize_wiki_url(&result.fullurl);

            Ok(PrtsOperatorDefinition {
                operator_id: operator_id.clone(),
                name_zh: result.fulltext.clone(),
                rarity,
                profession: profession.clone(),
                branch: branch.clone(),
                categories: categories.clone(),
                is_box_collectible,
                page_title: result.fulltext.clone(),
                raw_json: json!({
                    "operator_id": operator_id,
                    "name_zh": result.fulltext,
                    "rarity": rarity,
                    "profession": profession,
                    "branch": branch,
                    "categories": categories,
                    "availability_kind": availability_kind,
                    "is_box_collectible": is_box_collectible,
                    "page_title": result.fulltext,
                    "page_url": page_url,
                }),
            })
        })
        .collect()
}

fn extract_stage_definitions(
    payload: &PrtsStageIndexEnvelope,
) -> Result<Vec<PrtsStageDefinition>, PrtsClientError> {
    if payload.query.results.is_empty() {
        return Err(PrtsClientError::ParseStageIndexPayload {
            message: "stage index query returned no results".to_string(),
        });
    }

    payload
        .query
        .results
        .values()
        .map(|result| {
            let stage_id = result.printouts.stage_ids.first().cloned().ok_or_else(|| {
                PrtsClientError::ParseStageIndexPayload {
                    message: format!("stage page {} is missing 关卡id", result.fulltext),
                }
            })?;
            let categories = result
                .printouts
                .categories
                .iter()
                .map(|category| {
                    category
                        .fulltext
                        .strip_prefix("分类:")
                        .unwrap_or(category.fulltext.as_str())
                        .to_string()
                })
                .collect::<Vec<_>>();
            let code = derive_stage_code(&result.fulltext);
            let page_url = normalize_wiki_url(&result.fullurl);

            Ok(PrtsStageDefinition {
                stage_id: stage_id.clone(),
                zone_id: None,
                code: code.clone(),
                page_title: result.fulltext.clone(),
                categories: categories.clone(),
                raw_json: json!({
                    "stage_id": stage_id,
                    "code": code,
                    "page_title": result.fulltext,
                    "page_url": page_url,
                    "categories": categories,
                }),
            })
        })
        .collect()
}

fn extract_recipe_definitions(html: &str) -> Result<Vec<PrtsRecipeDefinition>, PrtsClientError> {
    let document = Html::parse_document(html);
    let table_selector = parse_html_selector("table.wikitable.logo")?;
    let row_selector = parse_html_selector("tr")?;
    let cell_selector = parse_html_selector("td")?;
    let section_selector = parse_html_selector("th[colspan=\"7\"]")?;

    let mut recipes = Vec::new();

    for table in document.select(&table_selector) {
        let header_text = normalize_text(&table.text().collect::<Vec<_>>().join(" "));
        if !(header_text.contains("加工站等级")
            && header_text.contains("所需原料")
            && header_text.contains("产品"))
        {
            continue;
        }

        let mut current_kind = None;
        for row in table.select(&row_selector) {
            if let Some(section) = row.select(&section_selector).next() {
                let section_name = normalize_text(&section.text().collect::<Vec<_>>().join(" "));
                if !section_name.is_empty() {
                    current_kind = Some(section_name);
                }
                continue;
            }

            let cells = row.select(&cell_selector).collect::<Vec<_>>();
            if cells.len() != 7 {
                continue;
            }

            let workshop_level = parse_i64_text(&normalize_text(
                &cells[0].text().collect::<Vec<_>>().join(" "),
            ))
            .ok_or_else(|| PrtsClientError::ParseRecipeIndexHtml {
                message: "recipe row is missing workshop level".to_string(),
            })?;
            let ingredients = extract_recipe_items_from_cell(&cells[1])?;
            let output_items = extract_recipe_items_from_cell(&cells[2])?;
            let output_name_zh = output_items
                .first()
                .map(|item| item.item_name_zh.clone())
                .ok_or_else(|| PrtsClientError::ParseRecipeIndexHtml {
                    message: "recipe row is missing output item".to_string(),
                })?;
            let lmd_cost = parse_i64_text(&normalize_text(
                &cells[3].text().collect::<Vec<_>>().join(" "),
            ))
            .ok_or_else(|| PrtsClientError::ParseRecipeIndexHtml {
                message: format!("recipe {output_name_zh} is missing LMD cost"),
            })?;
            let mood_cost = parse_i64_text(&normalize_text(
                &cells[4].text().collect::<Vec<_>>().join(" "),
            ))
            .ok_or_else(|| PrtsClientError::ParseRecipeIndexHtml {
                message: format!("recipe {output_name_zh} is missing mood cost"),
            })?;
            let byproduct_rate = parse_percent_text(&normalize_text(
                &cells[5].text().collect::<Vec<_>>().join(" "),
            ));
            let unlock_condition =
                normalize_optional_text(&cells[6].text().collect::<Vec<_>>().join(" "));
            let recipe_kind = current_kind.clone().unwrap_or_else(|| "未分类".to_string());

            recipes.push(PrtsRecipeDefinition {
                recipe_kind: recipe_kind.clone(),
                workshop_level,
                output_name_zh: output_name_zh.clone(),
                ingredients: ingredients.clone(),
                lmd_cost,
                mood_cost,
                byproduct_rate,
                unlock_condition: unlock_condition.clone(),
                raw_json: json!({
                    "recipe_kind": recipe_kind,
                    "workshop_level": workshop_level,
                    "output_name_zh": output_name_zh,
                    "ingredients": ingredients.iter().map(|item| json!({
                        "item_name_zh": item.item_name_zh,
                        "count": item.count,
                    })).collect::<Vec<_>>(),
                    "lmd_cost": lmd_cost,
                    "mood_cost": mood_cost,
                    "byproduct_rate": byproduct_rate,
                    "unlock_condition": unlock_condition,
                }),
            });
        }
    }

    if recipes.is_empty() {
        return Err(PrtsClientError::ParseRecipeIndexHtml {
            message: "recipe index HTML does not contain any recipe rows".to_string(),
        });
    }

    Ok(recipes)
}

fn extract_operator_growth_definitions(
    operator: &PrtsOperatorDefinition,
    section_name: &str,
    html: &str,
) -> Result<Vec<PrtsOperatorGrowthDefinition>, PrtsClientError> {
    match section_name {
        PRTS_OPERATOR_ELITE_GROWTH_SECTION => {
            extract_operator_elite_growth_definitions(operator, html)
        }
        PRTS_OPERATOR_SKILL_GROWTH_SECTION => {
            extract_operator_skill_growth_definitions(operator, html)
        }
        _ => Err(PrtsClientError::ParseOperatorGrowthHtml {
            message: format!(
                "unsupported operator growth section `{section_name}` for {}",
                operator.page_title
            ),
        }),
    }
}

fn extract_operator_elite_growth_definitions(
    operator: &PrtsOperatorDefinition,
    html: &str,
) -> Result<Vec<PrtsOperatorGrowthDefinition>, PrtsClientError> {
    let document = Html::parse_document(html);
    let table_selector = parse_html_selector("table")?;
    let row_selector = parse_html_selector("tr")?;
    let cell_selector = parse_html_selector("th, td")?;

    let mut growths = Vec::new();
    for table in document.select(&table_selector) {
        for row in table.select(&row_selector) {
            let cells = row.select(&cell_selector).collect::<Vec<_>>();
            if cells.len() != 2
                || cells[0].value().name() != "th"
                || cells[1].value().name() != "td"
            {
                continue;
            }

            let stage_label = normalize_text(&cells[0].text().collect::<Vec<_>>().join(" "));
            if !stage_label.contains("精英阶段") {
                continue;
            }

            let stage_key = elite_stage_key(&stage_label)
                .unwrap_or_else(|| format!("elite_row_{}", growths.len() + 1));
            let materials = extract_growth_materials_from_cell(&cells[1])?;
            growths.push(build_operator_growth_definition(
                operator,
                OperatorGrowthDescriptor {
                    growth_kind: "elite_promotion",
                    stage_key,
                    stage_label,
                    material_slot_key: "promotion".to_string(),
                    material_slot_label: "精英化".to_string(),
                    unlock_condition: None,
                },
                &materials,
            ));
        }
    }

    if !growths.is_empty() {
        return Ok(growths);
    }

    let normalized_text =
        normalize_text(&document.root_element().text().collect::<Vec<_>>().join(" "));
    if normalized_text.contains("无法精英化") {
        Ok(Vec::new())
    } else {
        Err(PrtsClientError::ParseOperatorGrowthHtml {
            message: format!(
                "operator {} elite growth section does not contain any rows",
                operator.page_title
            ),
        })
    }
}

fn extract_operator_skill_growth_definitions(
    operator: &PrtsOperatorDefinition,
    html: &str,
) -> Result<Vec<PrtsOperatorGrowthDefinition>, PrtsClientError> {
    let document = Html::parse_document(html);
    let table_selector = parse_html_selector("table")?;
    let row_selector = parse_html_selector("tr")?;
    let cell_selector = parse_html_selector("th, td")?;
    let text_body = normalize_text(&document.root_element().text().collect::<Vec<_>>().join(" "));

    let Some(table) = document.select(&table_selector).next() else {
        if text_body.contains("没有技能") {
            return Ok(Vec::new());
        }

        return Err(PrtsClientError::ParseOperatorGrowthHtml {
            message: format!(
                "operator {} skill growth section does not contain a table",
                operator.page_title
            ),
        });
    };

    let mut current_mode = SkillGrowthMode::Upgrade;
    let mut current_unlock_condition = None;
    let mut mastery_slots = Vec::new();
    let mut growths = Vec::new();

    for row in table.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.is_empty() {
            continue;
        }

        let text_by_cell = cells
            .iter()
            .map(|cell| normalize_text(&cell.text().collect::<Vec<_>>().join(" ")))
            .collect::<Vec<_>>();
        let row_text = normalize_text(&text_by_cell.join(" "));
        let td_count = cells
            .iter()
            .filter(|cell| cell.value().name() == "td")
            .count();

        if td_count == 0 {
            if row_text == "技能升级" {
                current_mode = SkillGrowthMode::Upgrade;
                current_unlock_condition = None;
                mastery_slots.clear();
                continue;
            }

            if row_text.contains("专精训练") {
                current_mode = SkillGrowthMode::Mastery;
                current_unlock_condition = Some(row_text.clone());
                mastery_slots.clear();
                continue;
            }

            if row_text.contains("达到精英阶段") && row_text.contains("解锁") {
                current_unlock_condition = Some(row_text.clone());
                continue;
            }

            let slot_headers = text_by_cell
                .iter()
                .filter(|text| text.starts_with('第') && text.contains("技能"))
                .cloned()
                .collect::<Vec<_>>();
            if !slot_headers.is_empty() {
                mastery_slots = slot_headers;
            }
            continue;
        }

        let mut pair_index = 0_usize;
        let mut pending_label = None::<String>;
        for cell in cells {
            let cell_text = normalize_text(&cell.text().collect::<Vec<_>>().join(" "));
            match cell.value().name() {
                "th" => {
                    pending_label = Some(cell_text);
                }
                "td" => {
                    let Some(stage_label) = pending_label.take() else {
                        continue;
                    };
                    let materials = extract_growth_materials_from_cell(&cell)?;

                    match current_mode {
                        SkillGrowthMode::Upgrade => {
                            if !stage_label.contains('→') {
                                continue;
                            }

                            let stage_key =
                                skill_upgrade_stage_key(&stage_label).unwrap_or_else(|| {
                                    format!("skill_upgrade_row_{}", growths.len() + 1)
                                });
                            growths.push(build_operator_growth_definition(
                                operator,
                                OperatorGrowthDescriptor {
                                    growth_kind: "skill_upgrade",
                                    stage_key,
                                    stage_label,
                                    material_slot_key: "global".to_string(),
                                    material_slot_label: "通用".to_string(),
                                    unlock_condition: current_unlock_condition.clone(),
                                },
                                &materials,
                            ));
                        }
                        SkillGrowthMode::Mastery => {
                            let slot_index = pair_index + 1;
                            let slot_display = mastery_slots
                                .get(pair_index)
                                .cloned()
                                .unwrap_or_else(|| format!("第{slot_index}技能"));
                            let stage_key = mastery_stage_key(&stage_label)
                                .unwrap_or_else(|| format!("mastery_row_{}", growths.len() + 1));
                            growths.push(build_operator_growth_definition(
                                operator,
                                OperatorGrowthDescriptor {
                                    growth_kind: "skill_mastery",
                                    stage_key,
                                    stage_label,
                                    material_slot_key: format!("skill_{slot_index}"),
                                    material_slot_label: slot_display,
                                    unlock_condition: current_unlock_condition.clone(),
                                },
                                &materials,
                            ));
                            pair_index += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if !growths.is_empty() {
        Ok(growths)
    } else if text_body.contains("没有技能") {
        Ok(Vec::new())
    } else {
        Err(PrtsClientError::ParseOperatorGrowthHtml {
            message: format!(
                "operator {} skill growth section does not contain any rows",
                operator.page_title
            ),
        })
    }
}

fn build_operator_growth_definition(
    operator: &PrtsOperatorDefinition,
    descriptor: OperatorGrowthDescriptor,
    materials: &[PrtsGrowthMaterial],
) -> PrtsOperatorGrowthDefinition {
    PrtsOperatorGrowthDefinition {
        operator_id: operator.operator_id.clone(),
        stage_label: descriptor.stage_label.clone(),
        material_slot: descriptor.material_slot_label.clone(),
        raw_json: json!({
            "operator_id": operator.operator_id,
            "operator_name_zh": operator.name_zh,
            "page_title": operator.page_title,
            "growth_kind": descriptor.growth_kind,
            "stage_key": descriptor.stage_key,
            "stage_label": descriptor.stage_label,
            "material_slot_key": descriptor.material_slot_key,
            "material_slot_label": descriptor.material_slot_label,
            "unlock_condition": descriptor.unlock_condition,
            "materials": materials.iter().map(|material| json!({
                "item_name_zh": material.item_name_zh,
                "count": material.count,
            })).collect::<Vec<_>>(),
        }),
    }
}

fn extract_operator_building_skill_definitions(
    operator: &PrtsOperatorDefinition,
    html: &str,
) -> Result<Vec<PrtsOperatorBuildingSkillDefinition>, PrtsClientError> {
    let document = Html::parse_document(html);
    let table_selector = parse_html_selector("table")?;
    let row_selector = parse_html_selector("tr")?;
    let cell_selector = parse_html_selector("th, td")?;
    let image_selector = parse_html_selector("img")?;
    let text_body = normalize_text(&document.root_element().text().collect::<Vec<_>>().join(" "));

    let mut building_skills = Vec::new();
    let mut sort_order = 0_i64;

    for table in document.select(&table_selector) {
        let mut skill_slot_label = None::<String>;

        for row in table.select(&row_selector) {
            let cells = row.select(&cell_selector).collect::<Vec<_>>();
            if cells.is_empty() {
                continue;
            }

            let cell_texts = cells
                .iter()
                .map(|cell| normalize_text(&cell.text().collect::<Vec<_>>().join(" ")))
                .collect::<Vec<_>>();
            let row_text = normalize_text(&cell_texts.join(" "));
            let td_count = cells
                .iter()
                .filter(|cell| cell.value().name() == "td")
                .count();

            if td_count == 0 {
                if row_text.contains("条件")
                    && row_text.contains("房间")
                    && row_text.contains("描述")
                    && cell_texts.len() >= 3
                {
                    skill_slot_label = cell_texts.get(2).cloned();
                }
                continue;
            }

            if cells.len() < 5 {
                continue;
            }

            let condition_label = cell_texts[0].clone();
            let skill_name = cell_texts[2].clone();
            let room_type_label = cell_texts[3].clone();
            let description = normalize_optional_text(&cell_texts[4]);

            if condition_label.is_empty() || skill_name.is_empty() || room_type_label.is_empty() {
                continue;
            }

            sort_order += 1;
            let condition_key = building_skill_condition_key(&condition_label)
                .unwrap_or_else(|| format!("row_{sort_order}"));
            let room_type_key = building_skill_room_type_key(&room_type_label);
            let image = cells[1].select(&image_selector).next();
            let icon_alt = image
                .as_ref()
                .and_then(|image| image.value().attr("alt"))
                .map(ToOwned::to_owned);
            let icon_url = image
                .as_ref()
                .and_then(|image| image.value().attr("src"))
                .map(ToOwned::to_owned);

            building_skills.push(PrtsOperatorBuildingSkillDefinition {
                operator_id: operator.operator_id.clone(),
                room_type: room_type_key.clone(),
                skill_name: skill_name.clone(),
                raw_json: json!({
                    "operator_id": operator.operator_id,
                    "operator_name_zh": operator.name_zh,
                    "page_title": operator.page_title,
                    "condition_key": condition_key,
                    "condition_label": condition_label,
                    "skill_slot_label": skill_slot_label.clone(),
                    "room_type_key": room_type_key,
                    "room_type_label": room_type_label,
                    "skill_name": skill_name,
                    "description": description,
                    "icon_alt": icon_alt,
                    "icon_url": icon_url,
                    "sort_order": sort_order,
                }),
            });
        }
    }

    if !building_skills.is_empty() {
        Ok(building_skills)
    } else if text_body.contains("后勤技能") && text_body.contains("暂无") {
        Ok(Vec::new())
    } else {
        Err(PrtsClientError::ParseOperatorBuildingSkillHtml {
            message: format!(
                "operator {} building skill section does not contain any rows",
                operator.page_title
            ),
        })
    }
}

fn parse_item_definition(tag: &str) -> Result<PrtsItemDefinition, PrtsClientError> {
    let item_id = decode_html_attribute(&required_attribute(tag, "data-id").ok_or_else(|| {
        PrtsClientError::ParseItemIndexHtml {
            message: "smwdata entry is missing data-id".to_string(),
        }
    })?);
    let name_zh =
        decode_html_attribute(&required_attribute(tag, "data-name").ok_or_else(|| {
            PrtsClientError::ParseItemIndexHtml {
                message: format!("smwdata entry {item_id} is missing data-name"),
            }
        })?);
    let description =
        optional_attribute(tag, "data-description").map(|value| decode_html_attribute(&value));
    let usage = optional_attribute(tag, "data-usage").map(|value| decode_html_attribute(&value));
    let obtain_approach =
        optional_attribute(tag, "data-obtain_approach").map(|value| decode_html_attribute(&value));
    let file_url = optional_attribute(tag, "data-file").map(|value| decode_html_attribute(&value));
    let rarity = optional_attribute(tag, "data-rarity").and_then(|value| value.parse::<i64>().ok());
    let dark_background =
        optional_attribute(tag, "data-dark-background").map(|value| match value.as_str() {
            "yes" | "true" | "1" => true,
            "no" | "false" | "0" => false,
            _ => false,
        });

    let categories = optional_attribute(tag, "data-category")
        .map(|value| {
            decode_html_attribute(&value)
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.strip_prefix("分类:").unwrap_or(value).to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let item_type = categories
        .iter()
        .find(|category| category.as_str() != "道具")
        .cloned()
        .or_else(|| categories.first().cloned())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(PrtsItemDefinition {
        item_id: item_id.clone(),
        name_zh: name_zh.clone(),
        item_type: item_type.clone(),
        rarity,
        description: description.clone(),
        usage: usage.clone(),
        obtain_approach: obtain_approach.clone(),
        categories: categories.clone(),
        file_url: file_url.clone(),
        dark_background,
        raw_json: json!({
            "item_id": item_id,
            "name_zh": name_zh,
            "item_type": item_type,
            "rarity": rarity,
            "description": description,
            "usage": usage,
            "obtain_approach": obtain_approach,
            "categories": categories,
            "file_url": file_url,
            "dark_background": dark_background,
        }),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrtsGrowthMaterial {
    item_name_zh: String,
    count: i64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SkillGrowthMode {
    Upgrade,
    Mastery,
}

#[derive(Debug, Clone)]
struct OperatorGrowthDescriptor {
    growth_kind: &'static str,
    stage_key: String,
    stage_label: String,
    material_slot_key: String,
    material_slot_label: String,
    unlock_condition: Option<String>,
}

fn extract_growth_materials_from_cell(
    cell: &ElementRef<'_>,
) -> Result<Vec<PrtsGrowthMaterial>, PrtsClientError> {
    let item_selector = parse_html_selector("div")?;
    let link_selector = parse_html_selector("a[title]")?;
    let mut items = Vec::new();

    for item in cell.select(&item_selector) {
        let Some(link) = item.select(&link_selector).next() else {
            continue;
        };
        let Some(title) = link.value().attr("title") else {
            continue;
        };
        let count_text = normalize_text(&item.text().collect::<Vec<_>>().join(" "));
        let count = parse_material_count_text(&count_text).unwrap_or(1);
        items.push(PrtsGrowthMaterial {
            item_name_zh: title.to_string(),
            count,
        });
    }

    if items.is_empty() {
        return Err(PrtsClientError::ParseOperatorGrowthHtml {
            message: "operator growth cell does not contain any item anchors".to_string(),
        });
    }

    Ok(items)
}

fn extract_recipe_items_from_cell(
    cell: &ElementRef<'_>,
) -> Result<Vec<PrtsRecipeIngredient>, PrtsClientError> {
    let item_selector = parse_html_selector("div")?;
    let link_selector = parse_html_selector("a[title]")?;
    let mut items = Vec::new();

    for item in cell.select(&item_selector) {
        let Some(link) = item.select(&link_selector).next() else {
            continue;
        };
        let Some(title) = link.value().attr("title") else {
            continue;
        };
        let count = parse_i64_text(&normalize_text(&item.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or(1);
        items.push(PrtsRecipeIngredient {
            item_name_zh: title.to_string(),
            count,
        });
    }

    if items.is_empty() {
        return Err(PrtsClientError::ParseRecipeIndexHtml {
            message: "recipe cell does not contain any item anchors".to_string(),
        });
    }

    Ok(items)
}

fn build_stage_index_query(offset: Option<usize>) -> String {
    let mut query = format!("[[关卡id::+]]|?关卡id|?分类|limit={PRTS_STAGE_INDEX_QUERY_LIMIT}");
    if let Some(offset) = offset {
        query.push_str(&format!("|offset={offset}"));
    }
    query
}

fn build_operator_index_query(offset: Option<usize>) -> String {
    let mut query = format!(
        "[[干员id::+]]|?干员id|?稀有度|?职业|?分支|?分类|limit={PRTS_OPERATOR_INDEX_QUERY_LIMIT}"
    );
    if let Some(offset) = offset {
        query.push_str(&format!("|offset={offset}"));
    }
    query
}

fn operator_availability_kind(page_title: &str, categories: &[String]) -> &'static str {
    if categories.iter().any(|category| category == "专属干员") {
        "exclusive_mode"
    } else if page_title.starts_with("预备干员") {
        "reserve_operator"
    } else {
        "box_collectible"
    }
}

fn derive_stage_code(page_title: &str) -> String {
    let Some((candidate, _)) = page_title.split_once(' ') else {
        return page_title.to_string();
    };

    if candidate.starts_with('(') {
        return page_title.to_string();
    }

    if candidate.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '#' | '/' | '_' | '.')
    }) {
        candidate.to_string()
    } else {
        page_title.to_string()
    }
}

fn elite_stage_key(stage_label: &str) -> Option<String> {
    let compact = stage_label
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == '→')
        .collect::<String>();
    let values = compact
        .split('→')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.len() == 2 {
        Some(format!("elite_{}_{}", values[0], values[1]))
    } else {
        None
    }
}

fn skill_upgrade_stage_key(stage_label: &str) -> Option<String> {
    let values = stage_label
        .split('→')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.len() == 2 {
        Some(format!("skill_{}_{}", values[0], values[1]))
    } else {
        None
    }
}

fn mastery_stage_key(stage_label: &str) -> Option<String> {
    let level = stage_label
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if level.is_empty() {
        None
    } else {
        Some(format!("mastery_{level}"))
    }
}

fn normalize_wiki_url(url: &str) -> String {
    if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    }
}

fn parse_html_selector(selector: &str) -> Result<Selector, PrtsClientError> {
    Selector::parse(selector).map_err(|error| PrtsClientError::ParseHtmlSelector {
        message: format!("{selector}: {error}"),
    })
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_optional_text(text: &str) -> Option<String> {
    let normalized = normalize_text(text);
    if normalized.is_empty() || normalized == "－" || normalized == "-" {
        None
    } else {
        Some(normalized)
    }
}

fn parse_i64_text(text: &str) -> Option<i64> {
    let digits = text
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<i64>().ok()
    }
}

fn parse_material_count_text(text: &str) -> Option<i64> {
    let normalized = normalize_text(text).to_lowercase();
    if normalized.is_empty() {
        return None;
    }

    let has_ten_thousand_unit = normalized.ends_with('w') || normalized.ends_with('万');
    let digits = normalized
        .trim_end_matches('w')
        .trim_end_matches('万')
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }

    let value = digits.parse::<f64>().ok()?;
    let multiplier = if has_ten_thousand_unit { 10_000.0 } else { 1.0 };
    Some((value * multiplier).round() as i64)
}

fn building_skill_condition_key(condition_label: &str) -> Option<String> {
    let digits = condition_label
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();

    if condition_label.starts_with("精英") && !digits.is_empty() {
        Some(format!("elite_{digits}"))
    } else {
        None
    }
}

fn building_skill_room_type_key(room_type_label: &str) -> String {
    match room_type_label {
        "控制中枢" => "control_center",
        "贸易站" => "trading_post",
        "制造站" => "manufacturing_station",
        "发电站" => "power_plant",
        "会客室" => "reception_room",
        "办公室" => "office",
        "宿舍" => "dormitory",
        "加工站" => "workshop",
        "训练室" => "training_room",
        _ => room_type_label,
    }
    .to_string()
}

fn parse_percent_text(text: &str) -> Option<f64> {
    let normalized = text.trim().trim_end_matches('%');
    if normalized.is_empty() {
        None
    } else {
        normalized.parse::<f64>().ok().map(|value| value / 100.0)
    }
}

fn required_attribute(tag: &str, name: &str) -> Option<String> {
    optional_attribute(tag, name)
}

fn optional_attribute(tag: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn decode_html_attribute(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut cursor = 0;

    while let Some(relative_ampersand) = value[cursor..].find('&') {
        let ampersand = cursor + relative_ampersand;
        decoded.push_str(&value[cursor..ampersand]);

        let Some(relative_semicolon) = value[ampersand..].find(';') else {
            decoded.push_str(&value[ampersand..]);
            return decoded;
        };

        let semicolon = ampersand + relative_semicolon;
        let entity = &value[ampersand + 1..semicolon];
        if let Some(character) = decode_html_entity(entity) {
            decoded.push(character);
        } else {
            decoded.push_str(&value[ampersand..=semicolon]);
        }

        cursor = semicolon + 1;
    }

    decoded.push_str(&value[cursor..]);
    decoded
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "quot" => Some('"'),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "apos" => Some('\''),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

#[derive(Debug, Error)]
pub enum PrtsClientError {
    #[error("failed to build PRTS HTTP client: {source}")]
    BuildHttpClient { source: reqwest::Error },
    #[error("failed to build PRTS API request URL: {message}")]
    BuildRequestUrl { message: String },
    #[error("failed to send request to PRTS: {source}")]
    SendRequest { source: reqwest::Error },
    #[error("PRTS returned an unexpected HTTP status: {source}")]
    HttpStatus { source: reqwest::Error },
    #[error("failed to read PRTS response body: {source}")]
    ReadResponseBody { source: reqwest::Error },
    #[error("failed to parse PRTS response body: {source}")]
    ParseResponseBody { source: serde_json::Error },
    #[error("failed to serialize PRTS response body: {source}")]
    SerializeResponseBody { source: serde_json::Error },
    #[error("failed to parse PRTS HTML selector: {message}")]
    ParseHtmlSelector { message: String },
    #[error("failed to parse PRTS item index HTML: {message}")]
    ParseItemIndexHtml { message: String },
    #[error("failed to parse PRTS recipe index HTML: {message}")]
    ParseRecipeIndexHtml { message: String },
    #[error("failed to parse PRTS operator growth HTML: {message}")]
    ParseOperatorGrowthHtml { message: String },
    #[error("failed to parse PRTS operator building skill HTML: {message}")]
    ParseOperatorBuildingSkillHtml { message: String },
    #[error("failed to parse PRTS operator index payload: {message}")]
    ParseOperatorIndexPayload { message: String },
    #[error("failed to parse PRTS stage index payload: {message}")]
    ParseStageIndexPayload { message: String },
}

impl PrtsClientError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::SendRequest { .. } | Self::ReadResponseBody { .. } => true,
            Self::HttpStatus { source } => source
                .status()
                .is_some_and(|status| status.is_server_error() || status.as_u16() == 429),
            Self::BuildHttpClient { .. }
            | Self::BuildRequestUrl { .. }
            | Self::ParseResponseBody { .. }
            | Self::SerializeResponseBody { .. }
            | Self::ParseHtmlSelector { .. }
            | Self::ParseItemIndexHtml { .. }
            | Self::ParseRecipeIndexHtml { .. }
            | Self::ParseOperatorGrowthHtml { .. }
            | Self::ParseOperatorBuildingSkillHtml { .. }
            | Self::ParseOperatorIndexPayload { .. }
            | Self::ParseStageIndexPayload { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrtsClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

                let response = if attempt == 0 {
                    let body = r#"{"error":"gateway timeout"}"#;
                    format!(
                        "HTTP/1.1 504 Gateway Time-out\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else {
                    let body = r#"{"parse":{"title":"道具一览","pageid":1109,"revid":335500,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\" lang=\"zh-Hans-CN\" dir=\"ltr\"><div class=\"smwdata\" data-name=\"龙门币\" data-description=\"基础货币。\" data-usage=\"用于养成。\" data-obtain_approach=\"&#91;&#91;主线&#93;&#93;掉落\" data-rarity=\"3\" data-category=\"分类:道具, 分类:货币\" data-id=\"4001\" data-dark-background=\"no\" data-file=\"https&#58;//media.prts.wiki/item_4001.png\"></div></div>"}}}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let client = PrtsClient::with_urls(
            format!("http://{address}/api.php"),
            format!("http://{address}/items"),
        )
        .unwrap();
        let item_index = client.fetch_item_index().unwrap();

        assert_eq!(item_index.revision, "335500");
        assert_eq!(item_index.items.len(), 1);
        assert_eq!(request_count.load(Ordering::SeqCst), 2);

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_item_index_from_http_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"parse":{"title":"道具一览","pageid":1109,"revid":335500,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\" lang=\"zh-Hans-CN\" dir=\"ltr\"><div class=\"smwdata\" data-name=\"龙门币\" data-description=\"基础货币。\" data-usage=\"用于养成。\" data-obtain_approach=\"&#91;&#91;主线&#93;&#93;掉落\" data-rarity=\"3\" data-category=\"分类:道具, 分类:货币\" data-id=\"4001\" data-dark-background=\"no\" data-file=\"https&#58;//media.prts.wiki/item_4001.png\"></div><div class=\"smwdata\" data-name=\"双极纳米片\" data-description=\"高阶材料。\" data-usage=\"用于精二与专精。\" data-obtain_approach=\"加工站合成\" data-rarity=\"4\" data-category=\"分类:道具, 分类:养成材料\" data-id=\"30104\" data-dark-background=\"yes\" data-file=\"https&#58;//media.prts.wiki/item_30104.png\"></div></div>"}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = PrtsClient::with_urls(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
        )
        .unwrap();
        let item_index = client.fetch_item_index().unwrap();

        assert_eq!(item_index.revision, "335500");
        assert_eq!(item_index.items.len(), 2);
        assert_eq!(item_index.items[0].item_id, "4001");
        assert_eq!(item_index.items[0].item_type, "货币");
        assert_eq!(
            item_index.items[0].obtain_approach.as_deref(),
            Some("[[主线]]掉落")
        );
        assert_eq!(
            item_index.items[0].file_url.as_deref(),
            Some("https://media.prts.wiki/item_4001.png")
        );
        assert_eq!(item_index.items[1].dark_background, Some(true));

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_paginated_stage_index_from_http_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("action=parse") {
                    r#"{"parse":{"title":"关卡一览","pageid":2325,"revid":375661}}"#
                } else if request.contains("offset%3D500") {
                    r#"{"query":{"results":{"(ISW-DF ???)":{"printouts":{"关卡id":["ro4_b_9"],"分类":[{"fulltext":"分类:活动关卡"}]},"fulltext":"(ISW-DF ???)","fullurl":"//prts.wiki/w/(ISW-DF_%3F%3F%3F)"}}}}"#
                } else {
                    r#"{"query-continue-offset":500,"query":{"results":{"0-1 坍塌":{"printouts":{"关卡id":["main_00-01"],"分类":[{"fulltext":"分类:主线关卡"},{"fulltext":"分类:普通难度关卡"}]},"fulltext":"0-1 坍塌","fullurl":"//prts.wiki/w/0-1_%E5%9D%8D%E5%A1%8C"},"1-7 暴君":{"printouts":{"关卡id":["main_01-07"],"分类":[{"fulltext":"分类:主线关卡"},{"fulltext":"分类:普通难度关卡"}]},"fulltext":"1-7 暴君","fullurl":"//prts.wiki/w/1-7_%E6%9A%B4%E5%90%9B"}}}}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let client = PrtsClient::with_urls(
            format!("http://{address}/api.php?action=query"),
            format!("http://{address}/items"),
        )
        .unwrap();
        let stage_index = client.fetch_stage_index().unwrap();

        assert_eq!(stage_index.revision, "375661");
        assert_eq!(stage_index.stages.len(), 3);
        assert_eq!(stage_index.stages[0].stage_id, "main_00-01");
        assert_eq!(stage_index.stages[0].code, "0-1");
        assert_eq!(
            stage_index.stages[0].categories,
            vec!["主线关卡", "普通难度关卡"]
        );
        assert_eq!(stage_index.stages[1].stage_id, "main_01-07");
        assert_eq!(stage_index.stages[1].code, "1-7");
        assert_eq!(stage_index.stages[2].stage_id, "ro4_b_9");
        assert_eq!(stage_index.stages[2].code, "(ISW-DF ???)");
        assert!(stage_index.content_type.contains("application/json"));
        assert!(!stage_index.raw_body.is_empty());

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_paginated_operator_index_from_http_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("action=parse") {
                    r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                } else if request.contains("offset%3D500") {
                    r#"{"query":{"results":{"阿消":{"printouts":{"干员id":["char_149_scave"],"稀有度":["3"],"职业":["特种"],"分支":[{"fulltext":"推击手"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:特种干员"}]},"fulltext":"阿消","fullurl":"//prts.wiki/w/%E9%98%BF%E6%B6%88"}}}}"#
                } else {
                    r#"{"query-continue-offset":500,"query":{"results":{"12F":{"printouts":{"干员id":["char_009_12fce"],"稀有度":["1"],"职业":["术师"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"}]},"fulltext":"12F","fullurl":"//prts.wiki/w/12F"},"阿米娅":{"printouts":{"干员id":["char_002_amiya"],"稀有度":["4"],"职业":["术师"],"分支":[{"fulltext":"中坚术师"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"},{"fulltext":"分类:属于罗德岛的干员"}]},"fulltext":"阿米娅","fullurl":"//prts.wiki/w/%E9%98%BF%E7%B1%B3%E5%A8%85"},"预备干员-重装":{"printouts":{"干员id":["char_513_apionr"],"稀有度":["3"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"}]},"fulltext":"预备干员-重装","fullurl":"//prts.wiki/w/%E9%A2%84%E5%A4%87%E5%B9%B2%E5%91%98-%E9%87%8D%E8%A3%85"}}}}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let client = PrtsClient::with_urls(
            format!("http://{address}/api.php?action=query"),
            format!("http://{address}/items"),
        )
        .unwrap();
        let operator_index = client.fetch_operator_index().unwrap();

        assert_eq!(operator_index.revision, "335492");
        assert_eq!(operator_index.operators.len(), 4);
        let operator_12f = operator_index
            .operators
            .iter()
            .find(|operator| operator.operator_id == "char_009_12fce")
            .unwrap();
        assert_eq!(operator_12f.name_zh, "12F");
        assert_eq!(operator_12f.categories, vec!["干员", "术师干员"]);
        assert!(operator_12f.is_box_collectible);

        let amiya = operator_index
            .operators
            .iter()
            .find(|operator| operator.operator_id == "char_002_amiya")
            .unwrap();
        assert_eq!(amiya.branch.as_deref(), Some("中坚术师"));
        assert!(amiya.is_box_collectible);

        let reserve_defender = operator_index
            .operators
            .iter()
            .find(|operator| operator.operator_id == "char_513_apionr")
            .unwrap();
        assert_eq!(reserve_defender.name_zh, "预备干员-重装");
        assert_eq!(reserve_defender.profession, "重装");
        assert!(!reserve_defender.is_box_collectible);
        assert_eq!(
            reserve_defender.raw_json["availability_kind"],
            serde_json::json!("reserve_operator")
        );

        let shaw = operator_index
            .operators
            .iter()
            .find(|operator| operator.operator_id == "char_149_scave")
            .unwrap();
        assert_eq!(shaw.profession, "特种");
        assert_eq!(shaw.branch.as_deref(), Some("推击手"));
        assert!(operator_index.content_type.contains("application/json"));
        assert!(!operator_index.raw_body.is_empty());

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_operator_growth_from_section_endpoints() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..8 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                    r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                } else if request.contains("%E5%B9%B2%E5%91%98id") {
                    r#"{"query":{"results":{"12F":{"printouts":{"干员id":["char_009_12fce"],"稀有度":["1"],"职业":["术师"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"}]},"fulltext":"12F","fullurl":"//prts.wiki/w/12F"},"能天使":{"printouts":{"干员id":["char_103_angel"],"稀有度":["5"],"职业":["狙击"],"分支":[{"fulltext":"速射手"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:狙击干员"}]},"fulltext":"能天使","fullurl":"//prts.wiki/w/%E8%83%BD%E5%A4%A9%E4%BD%BF"},"预备干员-重装":{"printouts":{"干员id":["char_513_apionr"],"稀有度":["3"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"}]},"fulltext":"预备干员-重装","fullurl":"//prts.wiki/w/%E9%A2%84%E5%A4%87%E5%B9%B2%E5%91%98-%E9%87%8D%E8%A3%85"}}}}"#
                } else if request.contains("page=12F&prop=sections%7Crevid") {
                    r#"{"parse":{"title":"12F","pageid":1703,"revid":400001,"sections":[{"line":"精英化材料","index":"9"},{"line":"技能升级材料","index":"10"}]}}"#
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=sections%7Crevid")
                {
                    r#"{"parse":{"title":"能天使","pageid":1769,"revid":400002,"sections":[{"line":"精英化材料","index":"9"},{"line":"技能升级材料","index":"10"}]}}"#
                } else if request.contains("page=12F&prop=text&section=9") {
                    r#"{"parse":{"title":"12F","pageid":1703,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><p>该干员无法精英化</p></div>"}}}"#
                } else if request.contains("page=12F&prop=text&section=10") {
                    r#"{"parse":{"title":"12F","pageid":1703,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><p>该干员没有技能</p></div>"}}}"#
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=text&section=9") {
                    r#"{"parse":{"title":"能天使","pageid":1769,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table><tbody><tr><th>精英阶段0→1</th><td><div><a title=\"龙门币\"></a><span>3w</span></div><div><a title=\"狙击芯片\"></a><span>5</span></div></td></tr></tbody></table></div>"}}}"#
                } else {
                    r#"{"parse":{"title":"能天使","pageid":1769,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table><tbody><tr><th colspan=\"2\">技能升级</th></tr><tr><th>1→2</th><td><div><a title=\"技巧概要·卷1\"></a><span>5</span></div></td></tr><tr><th colspan=\"2\">达到精英阶段1后解锁</th></tr><tr><th>4→5</th><td><div><a title=\"技巧概要·卷2\"></a><span>8</span></div></td></tr><tr><th colspan=\"2\">专精训练(达到精英阶段2后解锁)</th></tr><tr><th colspan=\"2\">第1技能</th></tr><tr><th>等级1</th><td><div><a title=\"技巧概要·卷3\"></a><span>6</span></div></td></tr></tbody></table></div>"}}}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let client = PrtsClient::with_urls_and_recipe(
            format!("http://{address}/api.php"),
            format!("http://{address}/items"),
            format!("http://{address}/recipes"),
        )
        .unwrap();
        let growth = client.fetch_operator_growth().unwrap();

        assert_eq!(growth.revision, "400002");
        assert_eq!(growth.operators.len(), 2);
        assert!(
            growth
                .operators
                .iter()
                .all(|operator| operator.name_zh != "预备干员-重装")
        );
        assert_eq!(growth.growths.len(), 4);
        assert_eq!(growth.growths[0].operator_id, "char_103_angel");
        assert_eq!(growth.growths[0].stage_label, "精英阶段0→1");
        assert_eq!(growth.growths[0].material_slot, "精英化");
        assert_eq!(
            growth.growths[0].raw_json["materials"][0]["count"],
            serde_json::json!(30_000)
        );
        assert_eq!(growth.growths[1].stage_label, "1→2");
        assert_eq!(growth.growths[1].material_slot, "通用");
        assert_eq!(
            growth.growths[2].raw_json["unlock_condition"],
            serde_json::json!("达到精英阶段1后解锁")
        );
        assert_eq!(growth.growths[3].stage_label, "等级1");
        assert_eq!(growth.growths[3].material_slot, "第1技能");
        assert!(growth.content_type.contains("application/json"));
        assert!(!growth.raw_body.is_empty());

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_operator_building_skills_from_section_endpoints() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for _ in 0..6 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request_buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut request_buffer).unwrap();
                let request = String::from_utf8_lossy(&request_buffer[..bytes_read]);

                let body = if request.contains("page=%E5%B9%B2%E5%91%98%E4%B8%80%E8%A7%88") {
                    r#"{"parse":{"title":"干员一览","pageid":2101,"revid":335492}}"#
                } else if request.contains("%E5%B9%B2%E5%91%98id") {
                    r#"{"query":{"results":{"能天使":{"printouts":{"干员id":["char_103_angel"],"稀有度":["5"],"职业":["狙击"],"分支":[{"fulltext":"速射手"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:狙击干员"}]},"fulltext":"能天使","fullurl":"//prts.wiki/w/%E8%83%BD%E5%A4%A9%E4%BD%BF"},"阿米娅":{"printouts":{"干员id":["char_002_amiya"],"稀有度":["5"],"职业":["术师"],"分支":[{"fulltext":"中坚术师"}],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:术师干员"}]},"fulltext":"阿米娅","fullurl":"//prts.wiki/w/%E9%98%BF%E7%B1%B3%E5%A8%85"},"预备干员-重装":{"printouts":{"干员id":["char_513_apionr"],"稀有度":["3"],"职业":["重装"],"分支":[],"分类":[{"fulltext":"分类:干员"},{"fulltext":"分类:重装干员"}]},"fulltext":"预备干员-重装","fullurl":"//prts.wiki/w/%E9%A2%84%E5%A4%87%E5%B9%B2%E5%91%98-%E9%87%8D%E8%A3%85"}}}}"#
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=sections%7Crevid")
                {
                    r#"{"parse":{"title":"能天使","pageid":1769,"revid":400002,"sections":[{"line":"后勤技能","index":"8"}]}}"#
                } else if request.contains("page=%E9%98%BF%E7%B1%B3%E5%A8%85&prop=sections%7Crevid")
                {
                    r#"{"parse":{"title":"阿米娅","pageid":1751,"revid":400003,"sections":[{"line":"后勤技能","index":"8"}]}}"#
                } else if request.contains("page=%E8%83%BD%E5%A4%A9%E4%BD%BF&prop=text&section=8") {
                    r#"{"parse":{"title":"能天使","pageid":1769,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table class=\"wikitable logo\"><tbody><tr><th>条件</th><th>图标</th><th>技能1</th><th>房间</th><th>描述</th></tr><tr><td>精英0</td><td><img alt=\"企鹅物流·α\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_tra_spd1.png\" /></td><td>企鹅物流·α</td><td>贸易站</td><td>进驻贸易站时，订单获取效率+20%</td></tr><tr><td>精英2</td><td><img alt=\"物流专家\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_tra_spd3.png\" /></td><td>物流专家</td><td>贸易站</td><td>进驻贸易站时，订单获取效率+35%</td></tr></tbody></table></div>"}}}"#
                } else {
                    r#"{"parse":{"title":"阿米娅","pageid":1751,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\"><table class=\"wikitable logo\"><tbody><tr><th>条件</th><th>图标</th><th>技能1</th><th>房间</th><th>描述</th></tr><tr><td>精英0</td><td><img alt=\"合作协议\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_ctrl_t_spd.png\" /></td><td>合作协议</td><td>控制中枢</td><td>进驻控制中枢时，所有贸易站订单效率+7%</td></tr></tbody></table><table class=\"wikitable logo\"><tbody><tr><th>条件</th><th>图标</th><th>技能2</th><th>房间</th><th>描述</th></tr><tr><td>精英2</td><td><img alt=\"小提琴独奏\" src=\"//torappu.prts.wiki/assets/build_skill_icon/bskill_dorm_all2.png\" /></td><td>小提琴独奏</td><td>宿舍</td><td>进驻宿舍时，该宿舍内所有干员的心情每小时恢复+0.15</td></tr></tbody></table></div>"}}}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let client = PrtsClient::with_urls_and_recipe(
            format!("http://{address}/api.php"),
            format!("http://{address}/items"),
            format!("http://{address}/recipes"),
        )
        .unwrap();
        let building_skills = client.fetch_operator_building_skills().unwrap();

        assert_eq!(building_skills.revision, "400003");
        assert_eq!(building_skills.operators.len(), 2);
        assert_eq!(building_skills.building_skills.len(), 4);
        assert!(
            building_skills
                .operators
                .iter()
                .all(|operator| operator.name_zh != "预备干员-重装")
        );
        assert_eq!(
            building_skills.building_skills[0].operator_id,
            "char_103_angel"
        );
        assert_eq!(building_skills.building_skills[0].room_type, "trading_post");
        assert_eq!(building_skills.building_skills[0].skill_name, "企鹅物流·α");
        assert_eq!(
            building_skills.building_skills[0].raw_json["condition_label"],
            serde_json::json!("精英0")
        );
        assert_eq!(
            building_skills.building_skills[2].raw_json["room_type_label"],
            serde_json::json!("控制中枢")
        );
        assert_eq!(
            building_skills.building_skills[3].raw_json["skill_slot_label"],
            serde_json::json!("技能2")
        );
        assert!(building_skills.content_type.contains("application/json"));
        assert!(!building_skills.raw_body.is_empty());

        server.join().unwrap();
    }

    #[test]
    fn client_fetches_recipe_index_from_http_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request_buffer = [0_u8; 1024];
            let _ = stream.read(&mut request_buffer).unwrap();
            let body = r#"{"parse":{"title":"罗德岛基建/加工站","pageid":15788,"revid":342715,"text":{"*":"<div class=\"mw-content-ltr mw-parser-output\" lang=\"zh-Hans-CN\" dir=\"ltr\"><table class=\"wikitable logo\"><tbody><tr><th rowspan=\"2\">加工站等级</th><th rowspan=\"2\">所需原料</th><th rowspan=\"2\">产品</th><th colspan=\"2\">加工消耗</th><th rowspan=\"2\">副产物概率</th><th rowspan=\"2\">额外解锁条件</th></tr><tr><td>加工费用</td><td>心情消耗</td></tr><tr><th colspan=\"7\">精英材料</th></tr><tr><td>3</td><td><div style=\"display:inline-block;position:relative\"><span typeof=\"mw:File\"><a href=\"/w/%E5%BC%82%E9%93%81\" title=\"异铁\"><img src=\"https://media.prts.wiki/item_a.png\" /></a></span><span>3</span></div><div style=\"display:inline-block;position:relative\"><span typeof=\"mw:File\"><a href=\"/w/%E7%A0%94%E7%A3%A8%E7%9F%B3\" title=\"研磨石\"><img src=\"https://media.prts.wiki/item_b.png\" /></a></span><span>1</span></div></td><td><div style=\"display:inline-block;position:relative\"><span typeof=\"mw:File\"><a href=\"/w/%E5%BC%82%E9%93%81%E7%BB%84\" title=\"异铁组\"><img src=\"https://media.prts.wiki/item_c.png\" /></a></span><span>1</span></div></td><td>300</td><td>2</td><td>10%</td><td>－</td></tr></tbody></table></div>"}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = PrtsClient::with_urls_and_recipe(
            format!("http://{address}/siteinfo"),
            format!("http://{address}/items"),
            format!("http://{address}/recipes"),
        )
        .unwrap();
        let recipe_index = client.fetch_recipe_index().unwrap();

        assert_eq!(recipe_index.revision, "342715");
        assert_eq!(recipe_index.recipes.len(), 1);
        assert_eq!(recipe_index.recipes[0].recipe_kind, "精英材料");
        assert_eq!(recipe_index.recipes[0].workshop_level, 3);
        assert_eq!(recipe_index.recipes[0].output_name_zh, "异铁组");
        assert_eq!(recipe_index.recipes[0].ingredients.len(), 2);
        assert_eq!(recipe_index.recipes[0].ingredients[0].item_name_zh, "异铁");
        assert_eq!(recipe_index.recipes[0].ingredients[0].count, 3);
        assert_eq!(recipe_index.recipes[0].lmd_cost, 300);
        assert_eq!(recipe_index.recipes[0].mood_cost, 2);
        assert_eq!(recipe_index.recipes[0].byproduct_rate, Some(0.10));
        assert_eq!(recipe_index.recipes[0].unlock_condition, None);

        server.join().unwrap();
    }
}
