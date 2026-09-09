use super::{get_json, json, require_key};
use crate::http::{HttpClient, url_join, url_with_query};
use crate::rate_limit::TokenBucket;
use crate::score::score_match;
use async_trait::async_trait;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{AuthorRef, MetadataMatch, MetadataQuery, WorkMetadata};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use url::Url;

const NAME: &str = "comicvine";
const TYPES: &[ContentType] = &[ContentType::Comic, ContentType::GraphicNovel];
const SEARCH: &str = "https://comicvine.gamespot.com/api/search/";
const UA: &str = "curatarr/0.1 (https://github.com/Quireware/curatarr)";

pub struct ComicVine {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
    api_key: Option<String>,
}

impl ComicVine {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket, api_key: Option<&str>) -> Self {
        Self {
            http,
            limiter,
            api_key: api_key.map(str::to_string),
        }
    }

    fn headers() -> &'static [(&'static str, &'static str)] {
        &[("user-agent", UA)]
    }
}

#[async_trait]
impl MetadataProvider for ComicVine {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let key = require_key(NAME, &self.api_key)?;
        let q = query.title.clone().unwrap_or_default();
        let url = url_with_query(
            SEARCH,
            &[
                ("api_key", key),
                ("format", "json"),
                ("resources", "volume"),
                ("query", q.as_str()),
            ],
        )?;
        let body = get_json(
            &self.http,
            &self.limiter,
            &url,
            Self::headers(),
            NAME,
            "search",
        )
        .await?;
        Ok(body
            .get("results")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item_to_match(item, query))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let key = require_key(NAME, &self.api_key)?;
        let ExternalId::ComicvineVolume(vol) = id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let resource = format!("4050-{vol}");
        let mut url = url_join("https://comicvine.gamespot.com/api/volume/", &[&resource])?;
        url.query_pairs_mut()
            .append_pair("api_key", key)
            .append_pair("format", "json");
        let body = get_json(&self.http, &self.limiter, &url, Self::headers(), NAME, vol).await?;
        let results = body
            .get("results")
            .cloned()
            .ok_or_else(|| ProviderError::NotFound {
                provider: NAME.into(),
                id: vol.clone(),
            })?;
        item_to_work(&results)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        if self.api_key.as_deref().unwrap_or("").is_empty() {
            return Ok(ProviderHealth {
                available: false,
                latency_ms: None,
                last_error: Some("missing API key".into()),
            });
        }
        match self
            .search(&MetadataQuery {
                title: Some("batman".into()),
                ..MetadataQuery::default()
            })
            .await
        {
            Ok(_) => Ok(health(started, None)),
            Err(e) => Ok(health(started, Some(e.to_string()))),
        }
    }
}

fn item_to_match(item: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let id = json::get_i64(item, "id")
        .map(|n| n.to_string())
        .or_else(|| json::get_str(item, "id"))?;
    let title = json::get_str(item, "name")?;
    let authors = Vec::new();
    let year = json::get_str(item, "start_year")
        .as_deref()
        .and_then(|s| s.parse().ok());
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::ComicvineVolume(id),
        score: score_match(query, &title, &authors, year, None),
        title,
        authors,
        year,
        description: json::get_str(item, "deck"),
        cover_url: item
            .get("image")
            .and_then(|img| json::first_str_in(img, &["super_url", "medium_url"]))
            .and_then(|u| Url::parse(&u).ok()),
    })
}

fn item_to_work(item: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_i64(item, "id")
        .map(|n| n.to_string())
        .or_else(|| json::get_str(item, "id"))
        .ok_or_else(|| ProviderError::Parse {
            provider: NAME.into(),
            reason: "missing id".into(),
        })?;
    let title = json::get_str(item, "name").unwrap_or_else(|| id.clone());
    let publisher = item.get("publisher").and_then(|p| json::get_str(p, "name"));
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::ComicvineVolume(id.clone()),
        title,
        sort_title: None,
        original_language: None,
        original_pub_date: None,
        description: json::first_str_in(item, &["description", "deck"]),
        content_type: ContentType::Comic,
        age_rating: None,
        average_rating: None,
        authors: publisher
            .into_iter()
            .map(|name| AuthorRef {
                name,
                external_id: None,
            })
            .collect(),
        series: None,
        tags: vec![],
        identifiers: vec![ExternalId::ComicvineVolume(id)],
        isbn13: None,
        isbn10: None,
        page_count: json::get_u32(item, "count_of_issues"),
        cover_url: item
            .get("image")
            .and_then(|img| json::first_str_in(img, &["super_url", "medium_url"]))
            .and_then(|u| Url::parse(&u).ok()),
    })
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
