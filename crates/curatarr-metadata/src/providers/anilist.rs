use super::{json, post_json};
use crate::http::HttpClient;
use crate::rate_limit::TokenBucket;
use crate::score::score_match;
use async_trait::async_trait;
use chrono::NaiveDate;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{
    AuthorRef, MetadataMatch, MetadataQuery, SeriesRef, WorkMetadata,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;
use url::Url;

const NAME: &str = "anilist";
const TYPES: &[ContentType] = &[
    ContentType::Manga,
    ContentType::LightNovel,
    ContentType::Webtoon,
];
const ENDPOINT: &str = "https://graphql.anilist.co/";
const SEARCH_QUERY: &str = r#"
query ($search: String) {
  Page(page: 1, perPage: 10) {
    media(search: $search, type: MANGA) {
      id title { romaji english native }
      description startDate { year } averageScore
      genres coverImage { large }
      staff { nodes { name { full } } }
    }
  }
}
"#;
const FETCH_QUERY: &str = r#"
query ($id: Int) {
  Media(id: $id, type: MANGA) {
    id title { romaji english native }
    description startDate { year month day } averageScore
    genres coverImage { large }
    staff { nodes { name { full } } }
    volumes chapters
  }
}
"#;

pub struct AniList {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
}

impl AniList {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket) -> Self {
        Self { http, limiter }
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, ProviderError> {
        let url = Url::parse(ENDPOINT).map_err(|e| ProviderError::Request {
            provider: NAME.into(),
            reason: e.to_string(),
        })?;
        let body = json!({ "query": query, "variables": variables });
        post_json(
            &self.http,
            &self.limiter,
            &url,
            &[("content-type", "application/json")],
            &body,
            NAME,
            "graphql",
        )
        .await
    }
}

#[async_trait]
impl MetadataProvider for AniList {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let search = query.title.clone().unwrap_or_default();
        let body = self
            .graphql(SEARCH_QUERY, json!({ "search": search }))
            .await?;
        let media = body
            .pointer("/data/Page/media")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(media
            .iter()
            .filter_map(|m| media_to_match(m, query))
            .collect())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let ExternalId::AniListMedia(media_id) = *id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let body = self.graphql(FETCH_QUERY, json!({ "id": media_id })).await?;
        let media =
            body.pointer("/data/Media")
                .cloned()
                .ok_or_else(|| ProviderError::NotFound {
                    provider: NAME.into(),
                    id: media_id.to_string(),
                })?;
        media_to_work(&media)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        match self.graphql("query { Viewer { id } }", json!({})).await {
            Ok(_) => Ok(health(started, None)),
            Err(e) => Ok(health(started, Some(e.to_string()))),
        }
    }
}

fn media_title(media: &Value) -> Option<String> {
    let t = media.get("title")?;
    json::first_str_in(t, &["english", "romaji", "native"])
}

fn media_to_match(media: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let id = json::get_i64(media, "id").and_then(|n| i32::try_from(n).ok())?;
    let title = media_title(media)?;
    let authors = staff_names(media);
    let year = media
        .pointer("/startDate/year")
        .and_then(Value::as_i64)
        .and_then(|y| i32::try_from(y).ok());
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::AniListMedia(id),
        score: score_match(query, &title, &authors, year, None),
        title,
        authors,
        year,
        description: json::get_str(media, "description"),
        cover_url: media
            .pointer("/coverImage/large")
            .and_then(Value::as_str)
            .and_then(|u| Url::parse(u).ok()),
    })
}

fn media_to_work(media: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_i64(media, "id")
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| ProviderError::Parse {
            provider: NAME.into(),
            reason: "missing id".into(),
        })?;
    let title = media_title(media).unwrap_or_else(|| id.to_string());
    let year = media
        .pointer("/startDate/year")
        .and_then(Value::as_i64)
        .and_then(|y| i32::try_from(y).ok());
    let month = media
        .pointer("/startDate/month")
        .and_then(Value::as_i64)
        .and_then(|m| u32::try_from(m).ok())
        .unwrap_or(1);
    let day = media
        .pointer("/startDate/day")
        .and_then(Value::as_i64)
        .and_then(|d| u32::try_from(d).ok())
        .unwrap_or(1);
    let rating = json::get_i64(media, "averageScore").map(anilist_score);
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::AniListMedia(id),
        title,
        sort_title: None,
        original_language: Some("ja".into()),
        original_pub_date: year.and_then(|y| NaiveDate::from_ymd_opt(y, month, day)),
        description: json::get_str(media, "description"),
        content_type: ContentType::Manga,
        age_rating: None,
        average_rating: rating,
        authors: staff_names(media)
            .into_iter()
            .map(|name| AuthorRef {
                name,
                external_id: None,
            })
            .collect(),
        series: media_title(media).map(|title| SeriesRef {
            title,
            position: None,
            external_id: None,
        }),
        tags: json::str_list(media, "genres"),
        identifiers: vec![ExternalId::AniListMedia(id)],
        isbn13: None,
        isbn10: None,
        page_count: json::get_u32(media, "chapters"),
        cover_url: media
            .pointer("/coverImage/large")
            .and_then(Value::as_str)
            .and_then(|u| Url::parse(u).ok()),
    })
}

fn staff_names(media: &Value) -> Vec<String> {
    media
        .pointer("/staff/nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|n| {
                    n.pointer("/name/full")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn anilist_score(v: i64) -> f64 {
    f64::from(i32::try_from(v).unwrap_or(0)) / 20.0
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
