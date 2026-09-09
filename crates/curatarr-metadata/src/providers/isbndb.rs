use super::{get_json, json, require_key};
use crate::http::{HttpClient, url_join};
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

const NAME: &str = "isbndb";
const TYPES: &[ContentType] = &[ContentType::Book];
const BASE: &str = "https://api2.isbndb.com/";

pub struct IsbnDb {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
    api_key: Option<String>,
}

impl IsbnDb {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket, api_key: Option<&str>) -> Self {
        Self {
            http,
            limiter,
            api_key: api_key.map(str::to_string),
        }
    }

    fn auth(&self) -> Result<[(&'static str, &str); 1], ProviderError> {
        let key = require_key(NAME, &self.api_key)?;
        Ok([("authorization", key)])
    }
}

#[async_trait]
impl MetadataProvider for IsbnDb {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let headers = self.auth()?;
        let url = if let Some(isbn) = query.isbn.as_deref() {
            url_join(BASE, &["book", isbn])?
        } else {
            let title = query.title.as_deref().unwrap_or("the");
            url_join(BASE, &["books", title])?
        };
        let body = get_json(&self.http, &self.limiter, &url, &headers, NAME, "search").await?;
        Ok(books_from(&body, query))
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let headers = self.auth()?;
        let isbn = match id {
            ExternalId::IsbnDb(v) => v.as_str(),
            _ => {
                return Err(ProviderError::Unsupported {
                    provider: NAME.into(),
                    feature: format!("id {id:?}"),
                });
            }
        };
        let url = url_join(BASE, &["book", isbn])?;
        let body = get_json(&self.http, &self.limiter, &url, &headers, NAME, isbn).await?;
        let book = body.get("book").cloned().unwrap_or(body);
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
        match self.search(&MetadataQuery::default()).await {
            Ok(_) => Ok(health(started, None)),
            Err(e) => Ok(health(started, Some(e.to_string()))),
        }
    }
}

fn books_from(body: &Value, query: &MetadataQuery) -> Vec<MetadataMatch> {
    let books = body
        .get("books")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| body.get("book").cloned().map(|b| vec![b]))
        .unwrap_or_default();
    books
        .iter()
        .filter_map(|b| {
            let isbn = json::first_str_in(b, &["isbn13", "isbn"])?;
            let title = json::get_str(b, "title")?;
            let authors = json::str_list(b, "authors");
            Some(MetadataMatch {
                provider: NAME.into(),
                external_id: ExternalId::IsbnDb(isbn.clone()),
                score: score_match(query, &title, &authors, None, Some(&isbn)),
                title,
                authors,
                year: json::get_str(b, "date_published")
                    .as_deref()
                    .and_then(|s| s.get(..4))
                    .and_then(|y| y.parse().ok()),
                description: json::get_str(b, "synopsis"),
                cover_url: json::get_str(b, "image").and_then(|u| Url::parse(&u).ok()),
            })
        })
        .collect()
}

fn book_to_work(book: &Value) -> Result<WorkMetadata, ProviderError> {
    let isbn = json::first_str_in(book, &["isbn13", "isbn"]).unwrap_or_default();
    let title = json::get_str(book, "title").unwrap_or_else(|| isbn.clone());
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::IsbnDb(isbn.clone()),
        title,
        sort_title: json::get_str(book, "title_long"),
        original_language: json::get_str(book, "language"),
        original_pub_date: json::get_str(book, "date_published").and_then(|s| {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok().or_else(|| {
                s.parse::<i32>()
                    .ok()
                    .and_then(|y| NaiveDate::from_ymd_opt(y, 1, 1))
            })
        }),
        description: json::get_str(book, "synopsis"),
        content_type: ContentType::Book,
        age_rating: None,
        average_rating: None,
        authors: json::str_list(book, "authors")
            .into_iter()
            .map(|name| AuthorRef {
                name,
                external_id: None,
            })
            .collect(),
        series: None,
        tags: json::str_list(book, "subjects"),
        identifiers: vec![ExternalId::IsbnDb(isbn.clone())],
        isbn13: Some(isbn),
        isbn10: json::get_str(book, "isbn"),
        page_count: json::get_u32(book, "pages"),
        cover_url: json::get_str(book, "image").and_then(|u| Url::parse(&u).ok()),
    })
}

fn health(started: Instant, err: Option<String>) -> ProviderHealth {
    ProviderHealth {
        available: err.is_none(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        last_error: err,
    }
}
