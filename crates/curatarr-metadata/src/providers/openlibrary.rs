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
use curatarr_core::types::metadata::{
    AuthorRef, MetadataMatch, MetadataQuery, SeriesRef, WorkMetadata,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use url::Url;

const NAME: &str = "openlibrary";
const TYPES: &[ContentType] = &[ContentType::Book, ContentType::LightNovel];
const SEARCH: &str = "https://openlibrary.org/search.json";
const BASE: &str = "https://openlibrary.org/";

pub struct OpenLibrary {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
}

impl OpenLibrary {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket) -> Self {
        Self { http, limiter }
    }
}

#[async_trait]
impl MetadataProvider for OpenLibrary {
    fn name(&self) -> &str {
        NAME
    }

    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let url = search_url(query)?;
        let body = get_json(&self.http, &self.limiter, &url, &[], NAME, "search").await?;
        let docs = body.get("docs").and_then(Value::as_array).cloned();
        let mut matches = Vec::new();
        for doc in docs.unwrap_or_default() {
            if let Some(m) = doc_to_match(&doc, query) {
                matches.push(m);
            }
        }
        Ok(matches)
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let key = match id {
            ExternalId::OpenLibraryWork(k) | ExternalId::OpenLibraryEdition(k) => k.as_str(),
            _ => {
                return Err(ProviderError::Unsupported {
                    provider: NAME.into(),
                    feature: format!("id {id:?}"),
                });
            }
        };
        let url = work_url(key)?;
        let body = get_json(&self.http, &self.limiter, &url, &[], NAME, key).await?;
        work_from_json(&body, key)
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        let url = url_with_query(SEARCH, &[("q", "the"), ("limit", "1")])?;
        match get_json(&self.http, &self.limiter, &url, &[], NAME, "health").await {
            Ok(_) => Ok(ProviderHealth {
                available: true,
                latency_ms: millis(started),
                last_error: None,
            }),
            Err(e) => Ok(ProviderHealth {
                available: false,
                latency_ms: millis(started),
                last_error: Some(e.to_string()),
            }),
        }
    }
}

fn search_url(query: &MetadataQuery) -> Result<Url, ProviderError> {
    let mut pairs: Vec<(&str, &str)> = vec![("limit", "10")];
    if let Some(isbn) = query.isbn.as_deref() {
        pairs.push(("isbn", isbn));
    } else if let Some(title) = query.title.as_deref() {
        pairs.push(("title", title));
        if let Some(author) = query.authors.first() {
            pairs.push(("author", author.as_str()));
        }
    } else if let Some(author) = query.authors.first() {
        pairs.push(("author", author.as_str()));
    } else {
        pairs.push(("q", "*"));
    }
    url_with_query(SEARCH, &pairs)
}

fn work_url(key: &str) -> Result<Url, ProviderError> {
    let trimmed = key.trim_start_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    url_join(BASE, &parts)
}

fn doc_to_match(doc: &Value, query: &MetadataQuery) -> Option<MetadataMatch> {
    let key = json::get_str(doc, "key")?;
    let title = json::get_str(doc, "title")?;
    let authors = json::str_list(doc, "author_name");
    let year = json::get_i64(doc, "first_publish_year").and_then(|y| i32::try_from(y).ok());
    let isbn = json::str_list(doc, "isbn").into_iter().next();
    let score = score_match(query, &title, &authors, year, isbn.as_deref());
    let cover = json::get_i64(doc, "cover_i")
        .and_then(|id| Url::parse(&format!("https://covers.openlibrary.org/b/id/{id}-L.jpg")).ok());
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::OpenLibraryWork(key),
        title,
        authors,
        year,
        description: None,
        cover_url: cover,
        score,
    })
}

fn work_from_json(body: &Value, key: &str) -> Result<WorkMetadata, ProviderError> {
    let title = json::get_str(body, "title").unwrap_or_else(|| key.to_string());
    let description = json::as_nonempty(body.get("description"));
    let authors = body
        .get("authors")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    a.get("author")
                        .and_then(|x| json::get_str(x, "key"))
                        .or_else(|| json::get_str(a, "key"))
                        .map(|k| AuthorRef {
                            name: k.trim_start_matches("/authors/").to_string(),
                            external_id: Some(ExternalId::OpenLibraryWork(
                                k.trim_start_matches("/authors/").to_string(),
                            )),
                        })
                })
                .collect()
        })
        .unwrap_or_default();
    let series = json::str_list(body, "subject")
        .into_iter()
        .find(|s| s.to_ascii_lowercase().contains("series"))
        .map(|title| SeriesRef {
            title,
            position: None,
            external_id: None,
        });
    let date = json::get_str(body, "first_publish_date")
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .or_else(|| {
            json::get_i64(body, "first_publish_year")
                .and_then(|y| i32::try_from(y).ok())
                .and_then(|y| NaiveDate::from_ymd_opt(y, 1, 1))
        });
    let cover = body
        .get("covers")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_i64)
        .and_then(|id| Url::parse(&format!("https://covers.openlibrary.org/b/id/{id}-L.jpg")).ok());
    Ok(WorkMetadata {
        provider: NAME.into(),
        external_id: ExternalId::OpenLibraryWork(key.trim_start_matches('/').to_string()),
        title,
        sort_title: None,
        original_language: None,
        original_pub_date: date,
        description,
        content_type: ContentType::Book,
        age_rating: None,
        average_rating: None,
        authors,
        series,
        tags: json::str_list(body, "subjects"),
        identifiers: vec![ExternalId::OpenLibraryWork(
            key.trim_start_matches('/').to_string(),
        )],
        isbn13: None,
        isbn10: None,
        page_count: None,
        cover_url: cover,
    })
}

fn millis(started: Instant) -> Option<u64> {
    u64::try_from(started.elapsed().as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::MockHttp;
    use crate::rate_limit::TokenBucket;

    #[tokio::test]
    async fn search_parses_docs() {
        let url = search_url(&MetadataQuery {
            title: Some("Dune".into()),
            ..MetadataQuery::default()
        })
        .unwrap();
        let body = r#"{"docs":[{"key":"/works/OL45883W","title":"Dune","author_name":["Frank Herbert"],"first_publish_year":1965}]}"#;
        let http = Arc::new(MockHttp::default().get(url.as_str(), body));
        let provider = OpenLibrary::new(http, TokenBucket::new(60));
        let results = provider
            .search(&MetadataQuery {
                title: Some("Dune".into()),
                ..MetadataQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Dune");
        assert!(results[0].score > 0.5);
    }
}
