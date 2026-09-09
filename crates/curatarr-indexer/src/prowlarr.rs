use async_trait::async_trait;
use curatarr_core::error::IndexerError;
use curatarr_core::traits::indexer::{Indexer, IndexerHealth, IndexerProtocol};
use curatarr_core::types::release::{IndexerQuery, Release};
use curatarr_metadata::http::{HttpClient, HttpMethod, HttpRequest, check_status, url_join};
use serde_json::Value;
use std::sync::Arc;
use url::Url;

pub struct Prowlarr {
    http: Arc<dyn HttpClient>,
    base: Url,
    api_key: String,
}

impl Prowlarr {
    pub fn new(http: Arc<dyn HttpClient>, base: Url, api_key: impl Into<String>) -> Self {
        Self {
            http,
            base,
            api_key: api_key.into(),
        }
    }

    fn search_url(&self, query: &str) -> Result<Url, IndexerError> {
        let mut url = url_join(self.base.as_str(), &["api", "v1", "search"])?;
        url.query_pairs_mut().append_pair("query", query);
        Ok(url)
    }
}

#[async_trait]
impl Indexer for Prowlarr {
    fn name(&self) -> &str {
        "prowlarr"
    }

    fn protocol(&self) -> IndexerProtocol {
        IndexerProtocol::Newznab
    }

    async fn search(&self, query: &IndexerQuery) -> Result<Vec<Release>, IndexerError> {
        if query.query.trim().is_empty() {
            return Ok(vec![]);
        }
        let url = self.search_url(&query.query)?;
        let key = self.api_key.as_str();
        let headers = [("x-api-key", key)];
        let response = self
            .http
            .send(HttpRequest {
                method: HttpMethod::Get,
                url: &url,
                headers: &headers,
                json_body: None,
                form_body: None,
            })
            .await?;
        check_status("prowlarr", &response, "search")?;
        let body = response.json("prowlarr")?;
        Ok(parse_hits(&body))
    }

    async fn health_check(&self) -> Result<IndexerHealth, IndexerError> {
        let url = url_join(self.base.as_str(), &["ping"])?;
        let key = self.api_key.as_str();
        let headers = [("x-api-key", key)];
        match self
            .http
            .send(HttpRequest {
                method: HttpMethod::Get,
                url: &url,
                headers: &headers,
                json_body: None,
                form_body: None,
            })
            .await
        {
            Ok(resp) if (200..300).contains(&resp.status) => Ok(IndexerHealth {
                available: true,
                last_error: None,
            }),
            Ok(resp) => Ok(IndexerHealth {
                available: false,
                last_error: Some(format!("HTTP {}", resp.status)),
            }),
            Err(e) => Ok(IndexerHealth {
                available: false,
                last_error: Some(e.to_string()),
            }),
        }
    }
}

fn parse_hits(body: &Value) -> Vec<Release> {
    let items = body.as_array().cloned().unwrap_or_default();
    items.into_iter().filter_map(parse_hit).collect()
}

fn parse_hit(hit: Value) -> Option<Release> {
    let title = hit.get("title")?.as_str()?.to_string();
    let guid = hit
        .get("guid")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let indexer = hit
        .get("indexer")
        .and_then(Value::as_str)
        .unwrap_or("prowlarr")
        .to_string();
    let download_url = hit
        .get("downloadUrl")
        .and_then(Value::as_str)
        .and_then(|u| Url::parse(u).ok())?;
    let size_bytes = hit
        .get("size")
        .and_then(Value::as_u64)
        .or_else(|| {
            hit.get("size")
                .and_then(Value::as_i64)
                .and_then(|n| u64::try_from(n).ok())
        })
        .unwrap_or(0);
    let protocol = match hit.get("protocol").and_then(Value::as_str) {
        Some("torrent") => IndexerProtocol::Torznab,
        _ => IndexerProtocol::Newznab,
    };
    let info_hash = hit
        .get("infoHash")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(Release {
        title,
        guid,
        indexer,
        download_url,
        size_bytes,
        protocol,
        info_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_metadata::http::{MockHttp, url_with_query};

    #[tokio::test]
    async fn search_sends_api_key_and_parses_hits() {
        let base = Url::parse("http://prowlarr.test/").unwrap();
        let url =
            url_with_query("http://prowlarr.test/api/v1/search", &[("query", "Dune")]).unwrap();
        let body = r#"[{"title":"Dune EPUB","guid":"g1","indexer":"nzbgeek","downloadUrl":"http://x/d.nzb","size":1000,"protocol":"usenet"}]"#;
        let http = Arc::new(MockHttp::default().get(url.as_str(), body));
        let client = Prowlarr::new(http.clone(), base, "secret-key");
        let hits = client
            .search(&IndexerQuery {
                query: "Dune".into(),
                content_type: None,
            })
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Dune EPUB");
        let last = http.last().unwrap();
        assert_eq!(
            last.headers,
            vec![("x-api-key".into(), "secret-key".into())]
        );
    }

    #[tokio::test]
    async fn empty_query_returns_no_hits() {
        let http = Arc::new(MockHttp::default());
        let client = Prowlarr::new(http, Url::parse("http://prowlarr.test/").unwrap(), "k");
        let hits = client
            .search(&IndexerQuery {
                query: "  ".into(),
                content_type: None,
            })
            .await
            .unwrap();
        assert!(hits.is_empty());
    }
}
