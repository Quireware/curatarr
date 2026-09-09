use super::{json, post_json};
use crate::http::HttpClient;
use crate::rate_limit::TokenBucket;
use crate::score::score_match;
use async_trait::async_trait;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{AuthorRef, MetadataMatch, MetadataQuery, WorkMetadata};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;
use url::Url;

const NAME: &str = "mangaupdates";
const TYPES: &[ContentType] = &[ContentType::Manga];
const SEARCH: &str = "https://api.mangaupdates.com/v1/series/search";
const SERIES: &str = "https://api.mangaupdates.com/v1/series/";

pub struct MangaUpdates {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
}

impl MangaUpdates {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket) -> Self {
        Self { http, limiter }
    }
}

#[async_trait]
impl MetadataProvider for MangaUpdates {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let url = Url::parse(SEARCH).map_err(url_err)?;
        let body = post_json(
            &self.http,
            &self.limiter,
            &url,
            &[("content-type", "application/json")],
            &json!({ "search": query.title.clone().unwrap_or_default(), "perpage": 10 }),
            NAME,
            "search",
        )
        .await?;
        Ok(body
            .get("results")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|row| {
                        let rec = row.get("record").unwrap_or(row);
                        record_to_match(rec, query)
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let ExternalId::MangaUpdatesSeries(sid) = id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let url = Url::parse(SERIES)
            .map_err(url_err)?
            .join(sid)
            .map_err(url_err)?;
        let body = super::get_json(&self.http, &self.limiter, &url, &[], NAME, sid).await?;
        record_to_work(&body)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
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

fn record_to_match(rec: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let id = json::get_i64(rec, "series_id")
        .map(|n| n.to_string())
        .or_else(|| json::get_str(rec, "series_id"))?;
    let title = json::get_str(rec, "title")?;
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::MangaUpdatesSeries(id),
        score: score_match(query, &title, &[], None, None),
        title,
        authors: vec![],
        year: json::get_i64(rec, "year").and_then(|y| i32::try_from(y).ok()),
        description: json::get_str(rec, "description"),
        cover_url: json::get_str(rec, "image").and_then(|u| Url::parse(&u).ok()),
    })
}

fn record_to_work(rec: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_i64(rec, "series_id")
        .map(|n| n.to_string())
        .or_else(|| json::get_str(rec, "series_id"))
        .ok_or_else(|| ProviderError::Parse {
            provider: NAME.into(),
            reason: "missing series_id".into(),
        })?;
    let title = json::get_str(rec, "title").unwrap_or_else(|| id.clone());
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::MangaUpdatesSeries(id.clone()),
        title,
        sort_title: None,
        original_language: None,
        original_pub_date: None,
        description: json::get_str(rec, "description"),
        content_type: ContentType::Manga,
        age_rating: None,
        average_rating: json::get_f64(rec, "bayesian_rating"),
        authors: rec
            .get("authors")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| json::get_str(a, "name"))
                    .map(|name| AuthorRef {
                        name,
                        external_id: None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        series: None,
        tags: json::str_list(rec, "genres"),
        identifiers: vec![ExternalId::MangaUpdatesSeries(id)],
        isbn13: None,
        isbn10: None,
        page_count: None,
        cover_url: rec
            .get("image")
            .and_then(|img| json::first_str_in(img, &["url", "thumb_url"]))
            .and_then(|u| Url::parse(&u).ok()),
    })
}

fn url_err(e: impl ToString) -> ProviderError {
    ProviderError::Request {
        provider: NAME.into(),
        reason: e.to_string(),
    }
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
