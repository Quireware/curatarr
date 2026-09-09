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

const NAME: &str = "myanimelist";
const TYPES: &[ContentType] = &[ContentType::Manga, ContentType::LightNovel];
const SEARCH: &str = "https://api.myanimelist.net/v2/manga";

pub struct MyAnimeList {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
    api_key: Option<String>,
}

impl MyAnimeList {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket, api_key: Option<&str>) -> Self {
        Self {
            http,
            limiter,
            api_key: api_key.map(str::to_string),
        }
    }

    fn headers(&self) -> Result<[(&'static str, &str); 1], ProviderError> {
        let key = require_key(NAME, &self.api_key)?;
        Ok([("x-mal-client-id", key)])
    }
}

#[async_trait]
impl MetadataProvider for MyAnimeList {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let headers = self.headers()?;
        let q = query.title.clone().unwrap_or_else(|| "a".into());
        let url = url_with_query(
            SEARCH,
            &[
                ("q", q.as_str()),
                ("limit", "10"),
                (
                    "fields",
                    "id,title,synopsis,start_date,mean,authors{node{name}},main_picture",
                ),
            ],
        )?;
        let body = get_json(&self.http, &self.limiter, &url, &headers, NAME, "search").await?;
        Ok(body
            .get("data")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|row| row.get("node").and_then(|n| node_to_match(n, query)))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let headers = self.headers()?;
        let ExternalId::MyAnimeList(mid) = *id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let mut url = url_join("https://api.myanimelist.net/v2/manga/", &[&mid.to_string()])?;
        url.query_pairs_mut().append_pair(
            "fields",
            "id,title,synopsis,start_date,mean,authors{node{name}},main_picture,num_volumes,genres",
        );
        let body = get_json(
            &self.http,
            &self.limiter,
            &url,
            &headers,
            NAME,
            &mid.to_string(),
        )
        .await?;
        node_to_work(&body)
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
        match self
            .search(&MetadataQuery {
                title: Some("naruto".into()),
                ..MetadataQuery::default()
            })
            .await
        {
            Ok(_) => Ok(health(started, None)),
            Err(e) => Ok(health(started, Some(e.to_string()))),
        }
    }
}

fn node_to_match(node: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let id = json::get_i64(node, "id").and_then(|n| i32::try_from(n).ok())?;
    let title = json::get_str(node, "title")?;
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::MyAnimeList(id),
        score: score_match(query, &title, &[], None, None),
        title,
        authors: vec![],
        year: json::get_str(node, "start_date")
            .as_deref()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse().ok()),
        description: json::get_str(node, "synopsis"),
        cover_url: node
            .pointer("/main_picture/large")
            .and_then(Value::as_str)
            .and_then(|u| Url::parse(u).ok()),
    })
}

fn node_to_work(node: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_i64(node, "id")
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| ProviderError::Parse {
            provider: NAME.into(),
            reason: "missing id".into(),
        })?;
    let title = json::get_str(node, "title").unwrap_or_else(|| id.to_string());
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::MyAnimeList(id),
        title,
        sort_title: None,
        original_language: Some("ja".into()),
        original_pub_date: None,
        description: json::get_str(node, "synopsis"),
        content_type: ContentType::Manga,
        age_rating: None,
        average_rating: json::get_f64(node, "mean"),
        authors: node
            .get("authors")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| {
                        a.pointer("/node/name")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .map(|name| AuthorRef {
                        name,
                        external_id: None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        series: None,
        tags: node
            .get("genres")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| json::get_str(g, "name"))
                    .collect()
            })
            .unwrap_or_default(),
        identifiers: vec![ExternalId::MyAnimeList(id)],
        isbn13: None,
        isbn10: None,
        page_count: json::get_u32(node, "num_volumes"),
        cover_url: node
            .pointer("/main_picture/large")
            .and_then(Value::as_str)
            .and_then(|u| Url::parse(u).ok()),
    })
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
