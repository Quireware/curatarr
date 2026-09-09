pub mod anilist;
pub mod comicvine;
pub mod goodreads;
pub mod google_books;
pub mod hardcover;
pub mod isbndb;
pub mod json;
pub mod librarything;
pub mod mangadex;
pub mod mangaupdates;
pub mod myanimelist;
pub mod openlibrary;

use crate::http::{HttpClient, HttpMethod, HttpRequest, HttpResponse, check_status};
use crate::rate_limit::TokenBucket;
use curatarr_core::error::ProviderError;
use serde_json::Value;
use std::sync::Arc;
use url::Url;

pub(crate) async fn get_json(
    http: &Arc<dyn HttpClient>,
    limiter: &TokenBucket,
    url: &Url,
    headers: &[(&str, &str)],
    provider: &str,
    id: &str,
) -> Result<Value, ProviderError> {
    limiter.acquire().await;
    let response = http
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers,
            json_body: None,
            form_body: None,
        })
        .await?;
    parse_json(provider, id, &response)
}

pub(crate) async fn post_json(
    http: &Arc<dyn HttpClient>,
    limiter: &TokenBucket,
    url: &Url,
    headers: &[(&str, &str)],
    body: &Value,
    provider: &str,
    id: &str,
) -> Result<Value, ProviderError> {
    limiter.acquire().await;
    let response = http
        .send(HttpRequest {
            method: HttpMethod::Post,
            url,
            headers,
            json_body: Some(body),
            form_body: None,
        })
        .await?;
    parse_json(provider, id, &response)
}

fn parse_json(provider: &str, id: &str, response: &HttpResponse) -> Result<Value, ProviderError> {
    check_status(provider, response, id)?;
    response.json(provider)
}

pub(crate) fn require_key<'a>(
    provider: &str,
    key: &'a Option<String>,
) -> Result<&'a str, ProviderError> {
    key.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderError::MissingApiKey {
            provider: provider.to_string(),
        })
}
