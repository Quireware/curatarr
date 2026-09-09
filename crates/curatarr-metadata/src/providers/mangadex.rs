use super::{get_json, json};
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

const NAME: &str = "mangadex";
const TYPES: &[ContentType] = &[ContentType::Manga, ContentType::Webtoon];
const API: &str = "https://api.mangadex.org/manga";

pub struct MangaDex {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
}

impl MangaDex {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket) -> Self {
        Self { http, limiter }
    }
}

#[async_trait]
impl MetadataProvider for MangaDex {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let title = query.title.clone().unwrap_or_default();
        let url = url_with_query(
            API,
            &[
                ("title", title.as_str()),
                ("limit", "10"),
                ("includes[]", "author"),
                ("includes[]", "cover_art"),
            ],
        )?;
        let body = get_json(&self.http, &self.limiter, &url, &[], NAME, "search").await?;
        Ok(body
            .get("data")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item_to_match(item, query))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let ExternalId::MangaDexSeries(uuid) = id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let mut url = url_join("https://api.mangadex.org/manga/", &[uuid])?;
        url.query_pairs_mut()
            .append_pair("includes[]", "author")
            .append_pair("includes[]", "artist")
            .append_pair("includes[]", "cover_art");
        let body = get_json(&self.http, &self.limiter, &url, &[], NAME, uuid).await?;
        let data = body
            .get("data")
            .cloned()
            .ok_or_else(|| ProviderError::NotFound {
                provider: NAME.into(),
                id: uuid.clone(),
            })?;
        item_to_work(&data)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        let url = url_with_query(API, &[("limit", "1")])?;
        match get_json(&self.http, &self.limiter, &url, &[], NAME, "health").await {
            Ok(_) => Ok(health(started, None)),
            Err(e) => Ok(health(started, Some(e.to_string()))),
        }
    }
}

fn title_of(item: &Value) -> Option<String> {
    let titles = item.pointer("/attributes/title")?;
    titles
        .get("en")
        .and_then(Value::as_str)
        .or_else(|| {
            titles
                .as_object()
                .and_then(|o| o.values().next().and_then(Value::as_str))
        })
        .and_then(json::nonempty)
}

fn item_to_match(item: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let id = json::get_str(item, "id")?;
    let title = title_of(item)?;
    let authors = rel_names(item, "author");
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::MangaDexSeries(id),
        score: score_match(query, &title, &authors, None, None),
        title,
        authors,
        year: item
            .pointer("/attributes/year")
            .and_then(Value::as_i64)
            .and_then(|y| i32::try_from(y).ok()),
        description: item
            .pointer("/attributes/description/en")
            .and_then(Value::as_str)
            .and_then(json::nonempty),
        cover_url: cover_url(item),
    })
}

fn item_to_work(item: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_str(item, "id").ok_or_else(|| ProviderError::Parse {
        provider: NAME.into(),
        reason: "missing id".into(),
    })?;
    let title = title_of(item).unwrap_or_else(|| id.clone());
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::MangaDexSeries(id.clone()),
        title,
        sort_title: None,
        original_language: item
            .pointer("/attributes/originalLanguage")
            .and_then(Value::as_str)
            .map(str::to_string),
        original_pub_date: None,
        description: item
            .pointer("/attributes/description/en")
            .and_then(Value::as_str)
            .and_then(json::nonempty),
        content_type: ContentType::Manga,
        age_rating: None,
        average_rating: None,
        authors: rel_names(item, "author")
            .into_iter()
            .map(|name| AuthorRef {
                name,
                external_id: None,
            })
            .collect(),
        series: None,
        tags: item
            .pointer("/attributes/tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(|t| {
                        t.pointer("/attributes/name/en")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        identifiers: vec![ExternalId::MangaDexSeries(id)],
        isbn13: None,
        isbn10: None,
        page_count: None,
        cover_url: cover_url(item),
    })
}

fn rel_names(item: &Value, rel: &str) -> Vec<String> {
    item.get("relationships")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|r| json::get_str(r, "type").as_deref() == Some(rel))
                .filter_map(|r| {
                    r.pointer("/attributes/name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cover_url(item: &Value) -> Option<Url> {
    let manga_id = json::get_str(item, "id")?;
    let file = item
        .get("relationships")
        .and_then(Value::as_array)?
        .iter()
        .find(|r| json::get_str(r, "type").as_deref() == Some("cover_art"))
        .and_then(|r| r.pointer("/attributes/fileName").and_then(Value::as_str))?;
    let mut url = Url::parse("https://uploads.mangadex.org").ok()?;
    {
        let mut segs = url.path_segments_mut().ok()?;
        segs.push("covers").push(&manga_id).push(file);
    }
    Some(url)
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
