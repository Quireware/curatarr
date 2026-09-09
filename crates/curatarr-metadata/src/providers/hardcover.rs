use super::{json, post_json, require_key};
use crate::http::HttpClient;
use crate::rate_limit::TokenBucket;
use crate::score::score_match;
use async_trait::async_trait;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{MetadataMatch, MetadataQuery, WorkMetadata};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;
use url::Url;

const NAME: &str = "hardcover";
const TYPES: &[ContentType] = &[ContentType::Book];
const ENDPOINT: &str = "https://api.hardcover.app/v1/graphql";
const SEARCH: &str = r#"
query ($q: String!) {
  search(query: $q, query_type: "Book", per_page: 10) {
    results
  }
}
"#;

pub struct Hardcover {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
    api_key: Option<String>,
}

impl Hardcover {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket, api_key: Option<&str>) -> Self {
        Self {
            http,
            limiter,
            api_key: api_key.map(str::to_string),
        }
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, ProviderError> {
        let key = require_key(NAME, &self.api_key)?;
        let url = Url::parse(ENDPOINT).map_err(|e| ProviderError::Request {
            provider: NAME.into(),
            reason: e.to_string(),
        })?;
        let auth = format!("Bearer {key}");
        let headers = [
            ("content-type", "application/json"),
            ("authorization", auth.as_str()),
        ];
        post_json(
            &self.http,
            &self.limiter,
            &url,
            &headers,
            &json!({ "query": query, "variables": variables }),
            NAME,
            "graphql",
        )
        .await
    }
}

#[async_trait]
impl MetadataProvider for Hardcover {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let q = query.title.clone().unwrap_or_default();
        let body = self.graphql(SEARCH, json!({ "q": q })).await?;
        let results = body.pointer("/data/search/results").cloned();
        Ok(extract_hits(&results.unwrap_or(Value::Null), query))
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let ExternalId::HardcoverId(hid) = id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let body = self
            .graphql(
                r#"query ($id: Int!) { books_by_pk(id: $id) { id title description cached_tags } }"#,
                json!({ "id": hid.parse::<i64>().unwrap_or(0) }),
            )
            .await?;
        let book =
            body.pointer("/data/books_by_pk")
                .cloned()
                .ok_or_else(|| ProviderError::NotFound {
                    provider: NAME.into(),
                    id: hid.clone(),
                })?;
        book_to_work(&book)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        if require_key(NAME, &self.api_key).is_err() {
            return Ok(ProviderHealth {
                available: false,
                latency_ms: None,
                last_error: Some("missing API key".into()),
            });
        }
        match self.graphql("query { me { id } }", json!({})).await {
            Ok(_) => Ok(health(started, None)),
            Err(e) => Ok(health(started, Some(e.to_string()))),
        }
    }
}

fn extract_hits(results: &Value, query: &MetadataQuery) -> Vec<MetadataMatch> {
    let hits = results
        .get("hits")
        .and_then(Value::as_array)
        .or_else(|| results.as_array());
    hits.map(|arr| {
        arr.iter()
            .filter_map(|hit| {
                let doc = hit.get("document").unwrap_or(hit);
                let id = json::get_i64(doc, "id")
                    .map(|n| n.to_string())
                    .or_else(|| json::get_str(doc, "id"))?;
                let title = json::get_str(doc, "title")?;
                Some(MetadataMatch {
                    provider: NAME.into(),
                    external_id: ExternalId::HardcoverId(id),
                    score: score_match(query, &title, &[], None, None),
                    title,
                    authors: vec![],
                    year: json::get_i64(doc, "release_year").and_then(|y| i32::try_from(y).ok()),
                    description: json::get_str(doc, "description"),
                    cover_url: json::get_str(doc, "image").and_then(|u| Url::parse(&u).ok()),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

fn book_to_work(book: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_i64(book, "id")
        .map(|n| n.to_string())
        .or_else(|| json::get_str(book, "id"))
        .unwrap_or_default();
    let title = json::get_str(book, "title").unwrap_or_else(|| id.clone());
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::HardcoverId(id.clone()),
        title,
        sort_title: None,
        original_language: None,
        original_pub_date: None,
        description: json::get_str(book, "description"),
        content_type: ContentType::Book,
        age_rating: None,
        average_rating: json::get_f64(book, "rating"),
        authors: vec![],
        series: None,
        tags: vec![],
        identifiers: vec![ExternalId::HardcoverId(id)],
        isbn13: json::get_str(book, "isbn_13"),
        isbn10: json::get_str(book, "isbn_10"),
        page_count: json::get_u32(book, "pages"),
        cover_url: None,
    })
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
