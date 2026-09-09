use super::require_key;
use crate::http::{HttpClient, HttpMethod, HttpRequest, check_status, url_with_query};
use crate::rate_limit::TokenBucket;
use crate::score::score_match;
use async_trait::async_trait;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{MetadataMatch, MetadataQuery, WorkMetadata};
use std::sync::Arc;

const NAME: &str = "librarything";
const TYPES: &[ContentType] = &[ContentType::Book];
const REST: &str = "https://www.librarything.com/services/rest/1.1/";

pub struct LibraryThing {
    http: Arc<dyn HttpClient>,
    limiter: TokenBucket,
    api_key: Option<String>,
}

impl LibraryThing {
    pub fn new(http: Arc<dyn HttpClient>, limiter: TokenBucket, api_key: Option<&str>) -> Self {
        Self {
            http,
            limiter,
            api_key: api_key.map(str::to_string),
        }
    }

    async fn get_xml(&self, isbn: &str) -> Result<String, ProviderError> {
        let key = require_key(NAME, &self.api_key)?;
        let url = url_with_query(
            REST,
            &[
                ("method", "librarything.ck.getwork"),
                ("isbn", isbn),
                ("apikey", key),
            ],
        )?;
        self.limiter.acquire().await;
        let response = self
            .http
            .send(HttpRequest {
                method: HttpMethod::Get,
                url: &url,
                headers: &[],
                json_body: None,
                form_body: None,
            })
            .await?;
        check_status(NAME, &response, isbn)?;
        response.text(NAME)
    }
}

#[async_trait]
impl MetadataProvider for LibraryThing {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        TYPES
    }

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        let Some(isbn) = query.isbn.as_deref() else {
            return Ok(vec![]);
        };
        let xml = self.get_xml(isbn).await?;
        Ok(xml_to_match(&xml, query).into_iter().collect())
    }

    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        let ExternalId::LibraryThingWork(wid) = id else {
            return Err(ProviderError::Unsupported {
                provider: NAME.into(),
                feature: format!("id {id:?}"),
            });
        };
        Ok(WorkMetadata {
            provider: NAME.into(),
            external_id: ExternalId::LibraryThingWork(wid.clone()),
            title: wid.clone(),
            sort_title: None,
            original_language: None,
            original_pub_date: None,
            description: None,
            content_type: ContentType::Book,
            age_rating: None,
            average_rating: None,
            authors: vec![],
            series: None,
            tags: vec![],
            identifiers: vec![ExternalId::LibraryThingWork(wid.clone())],
            isbn13: None,
            isbn10: None,
            page_count: None,
            cover_url: None,
        })
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        Ok(ProviderHealth {
            available: require_key(NAME, &self.api_key).is_ok(),
            latency_ms: None,
            last_error: require_key(NAME, &self.api_key)
                .err()
                .map(|e| e.to_string()),
        })
    }
}

fn xml_to_match(xml: &str, query: &MetadataQuery) -> Option<MetadataMatch> {
    let title = xml_tag(xml, "title")?;
    let author = xml_tag(xml, "author");
    let id = xml_tag(xml, "url")
        .or_else(|| xml_attr(xml, "item", "id"))
        .unwrap_or_else(|| title.clone());
    let authors: Vec<String> = author.into_iter().collect();
    Some(MetadataMatch {
        provider: NAME.into(),
        external_id: ExternalId::LibraryThingWork(id),
        score: score_match(query, &title, &authors, None, query.isbn.as_deref()),
        title,
        authors,
        year: None,
        description: xml_tag(xml, "commonknowledge"),
        cover_url: None,
    })
}

fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let inner = xml[start..end].trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

fn xml_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let needle = format!("<{tag}");
    let start = xml.find(&needle)?;
    let rest = &xml[start..];
    let attr_key = format!("{attr}=\"");
    let a = rest.find(&attr_key)? + attr_key.len();
    let b = rest[a..].find('"')? + a;
    Some(rest[a..b].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_tag_extracts_title() {
        let xml = "<response><title>Dune</title><author>Herbert</author></response>";
        assert_eq!(xml_tag(xml, "title").as_deref(), Some("Dune"));
        let q = MetadataQuery {
            title: Some("Dune".into()),
            isbn: Some("9780306406157".into()),
            ..MetadataQuery::default()
        };
        let m = xml_to_match(xml, &q).unwrap();
        assert_eq!(m.title, "Dune");
    }
}
