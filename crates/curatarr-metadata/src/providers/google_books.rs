use super::{get_json, json};
use crate::http::{HttpClient, url_join, url_with_query};
use crate::rate_limit::TokenBucket;
use crate::score::score_match;
use async_trait::async_trait;
use chrono::NaiveDate;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{AuthorRef, MetadataMatch, MetadataQuery, WorkMetadata};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use url::Url;

const NAME: &str = "googlebooks";
const TYPES: &[ContentType] = &[ContentType::Book, ContentType::LightNovel];
const SEARCH: &str = "https://www.googleapis.com/books/v1/volumes";

pub struct GoogleBooks {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
    api_key: Option<String>,
}

impl GoogleBooks {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket, api_key: Option<&str>) -> Self {
        Self {
            http,
            limiter,
            api_key: api_key.map(str::to_string),
        }
    }
}

#[async_trait]
impl MetadataProvider for GoogleBooks {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let q = search_q(query);
        let url = self.list_url(&q)?;
        let body = get_json(&self.http, &self.limiter, &url, &[], NAME, "search").await?;
        Ok(body
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item_to_match(item, query))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let ExternalId::GoogleBooksVolume(vol) = id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        let url = self.volume_url(vol)?;
        let body = get_json(&self.http, &self.limiter, &url, &[], NAME, vol).await?;
        item_to_work(&body)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        let url = self.list_url("the")?;
        match get_json(&self.http, &self.limiter, &url, &[], NAME, "health").await {
            Ok(_) => Ok(ok_health(started)),
            Err(e) => Ok(err_health(started, e)),
        }
    }
}

impl GoogleBooks {
    fn list_url(&self, q: &str) -> Result<Url, ProviderError> {
        let mut pairs = vec![("q", q), ("maxResults", "10")];
        if let Some(key) = self.api_key.as_deref() {
            pairs.push(("key", key));
        }
        url_with_query(SEARCH, &pairs)
    }

    fn volume_url(&self, id: &str) -> Result<Url, ProviderError> {
        let mut url = url_join("https://www.googleapis.com/books/v1/volumes/", &[id])?;
        if let Some(key) = self.api_key.as_deref() {
            url.query_pairs_mut().append_pair("key", key);
        }
        Ok(url)
    }
}

fn search_q(query: &MetadataQuery) -> String {
    if let Some(isbn) = query.isbn.as_deref() {
        let mut q = String::from("isbn:");
        q.push_str(isbn);
        return q;
    }
    let mut q = String::new();
    if let Some(title) = query.title.as_deref() {
        q.push_str("intitle:");
        q.push_str(title);
    }
    if let Some(author) = query.authors.first() {
        if !q.is_empty() {
            q.push(' ');
        }
        q.push_str("inauthor:");
        q.push_str(author);
    }
    if q.is_empty() {
        q.push_str("the");
    }
    q
}

fn item_to_match(item: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let id = json::get_str(item, "id")?;
    let info = item.get("volumeInfo")?;
    let title = json::get_str(info, "title")?;
    let authors = json::str_list(info, "authors");
    let year = json::get_str(info, "publishedDate")
        .as_deref()
        .and_then(|s| s.get(..4))
        .and_then(|y| y.parse().ok());
    let isbn = isbn_of(info);
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::GoogleBooksVolume(id),
        score: score_match(query, &title, &authors, year, isbn.as_deref()),
        title,
        authors,
        year,
        description: json::get_str(info, "description"),
        cover_url: info
            .get("imageLinks")
            .and_then(|l| json::get_str(l, "thumbnail"))
            .and_then(|u| Url::parse(&u).ok()),
    })
}

fn item_to_work(item: &Value) -> Result<WorkMetadata, ProviderError> {
    let id = json::get_str(item, "id").unwrap_or_default();
    let info = item.get("volumeInfo").cloned().unwrap_or(Value::Null);
    let title = json::get_str(&info, "title").unwrap_or_else(|| id.clone());
    let authors = json::str_list(&info, "authors")
        .into_iter()
        .map(|name| AuthorRef {
            name,
            external_id: None,
        })
        .collect();
    let date = json::get_str(&info, "publishedDate").and_then(|s| parse_year_date(&s));
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::GoogleBooksVolume(id),
        title,
        sort_title: None,
        original_language: json::get_str(&info, "language"),
        original_pub_date: date,
        description: json::get_str(&info, "description"),
        content_type: ContentType::Book,
        age_rating: None,
        average_rating: json::get_f64(&info, "averageRating"),
        authors,
        series: None,
        tags: json::str_list(&info, "categories"),
        identifiers: vec![],
        isbn13: isbn_of(&info),
        isbn10: None,
        page_count: json::get_u32(&info, "pageCount"),
        cover_url: info
            .get("imageLinks")
            .and_then(|l| json::first_str_in(l, &["large", "thumbnail"]))
            .and_then(|u| Url::parse(&u).ok()),
    })
}

fn isbn_of(info: &Value) -> Option<String> {
    info.get("industryIdentifiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|id| json::get_str(id, "type").as_deref() == Some("ISBN_13"))
        .and_then(|id| json::get_str(id, "identifier"))
}

fn parse_year_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().or_else(|| {
        s.parse::<i32>()
            .ok()
            .and_then(|y| NaiveDate::from_ymd_opt(y, 1, 1))
    })
}

fn ok_health(started: Instant) -> ProviderHealth {
    ProviderHealth {
        available: true,
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: None,
    }
}

fn err_health(started: Instant, e: ProviderError) -> ProviderHealth {
    ProviderHealth {
        available: false,
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: Some(e.to_string()),
    }
}
