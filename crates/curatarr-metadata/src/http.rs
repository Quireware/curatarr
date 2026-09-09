use async_trait::async_trait;
use curatarr_core::error::ProviderError;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

pub struct HttpRequest<'a> {
    pub method: HttpMethod,
    pub url: &'a Url,
    pub headers: &'a [(&'a str, &'a str)],
    pub json_body: Option<&'a Value>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn json(&self, provider: &str) -> Result<Value, ProviderError> {
        serde_json::from_slice(&self.body).map_err(|e| ProviderError::Parse {
            provider: provider.to_string(),
            reason: e.to_string(),
        })
    }

    pub fn text(&self, provider: &str) -> Result<String, ProviderError> {
        String::from_utf8(self.body.clone()).map_err(|e| ProviderError::Parse {
            // clone: UTF-8 conversion consumes a copy of the body bytes
            provider: provider.to_string(),
            reason: e.to_string(),
        })
    }
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, request: HttpRequest<'_>) -> Result<HttpResponse, ProviderError>;
}

pub struct ReqwestHttp {
    client: reqwest::Client,
}

impl ReqwestHttp {
    pub fn new() -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .user_agent("curatarr/0.1 (https://github.com/Quireware/curatarr)")
            .build()
            .map_err(|e| ProviderError::Request {
                provider: "http".into(),
                reason: e.to_string(),
            })?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpClient for ReqwestHttp {
    async fn send(&self, request: HttpRequest<'_>) -> Result<HttpResponse, ProviderError> {
        let mut builder = match request.method {
            HttpMethod::Get => self.client.get(request.url.clone()), // clone: reqwest takes owned Url
            HttpMethod::Post => self.client.post(request.url.clone()), // clone: reqwest takes owned Url
        };
        for (name, value) in request.headers {
            builder = builder.header(*name, *value);
        }
        if let Some(body) = request.json_body {
            builder = builder.json(body);
        }
        let response = builder.send().await.map_err(|e| ProviderError::Request {
            provider: "http".into(),
            reason: e.to_string(),
        })?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| ProviderError::Request {
                provider: "http".into(),
                reason: e.to_string(),
            })?
            .to_vec();
        Ok(HttpResponse { status, body })
    }
}

#[derive(Default)]
pub struct MockHttp {
    routes: Mutex<HashMap<String, HttpResponse>>,
}

impl MockHttp {
    pub fn get(self, url: &str, body: &str) -> Self {
        self.insert("GET", url, 200, body.as_bytes())
    }

    pub fn post(self, url: &str, body: &str) -> Self {
        self.insert("POST", url, 200, body.as_bytes())
    }

    pub fn status(self, method: &str, url: &str, status: u16, body: &str) -> Self {
        self.insert(method, url, status, body.as_bytes())
    }

    fn insert(self, method: &str, url: &str, status: u16, body: &[u8]) -> Self {
        let key = route_key(method, url);
        self.routes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                key,
                HttpResponse {
                    status,
                    body: body.to_vec(),
                },
            );
        self
    }
}

fn route_key(method: &str, url: &str) -> String {
    format!("{method} {url}")
}

#[async_trait]
impl HttpClient for MockHttp {
    async fn send(&self, request: HttpRequest<'_>) -> Result<HttpResponse, ProviderError> {
        let method = match request.method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
        };
        let key = route_key(method, request.url.as_str());
        self.routes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned() // clone: mock returns an owned copy of the canned response
            .ok_or_else(|| ProviderError::Request {
                provider: "mock".into(),
                reason: format!("no mock for {key}"),
            })
    }
}

pub fn url_with_query(base: &str, pairs: &[(&str, &str)]) -> Result<Url, ProviderError> {
    let mut url = Url::parse(base).map_err(|e| ProviderError::Request {
        provider: "http".into(),
        reason: e.to_string(),
    })?;
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    Ok(url)
}

pub fn url_join(base: &str, segments: &[&str]) -> Result<Url, ProviderError> {
    let mut url = Url::parse(base).map_err(|e| ProviderError::Request {
        provider: "http".into(),
        reason: e.to_string(),
    })?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|()| ProviderError::Request {
                provider: "http".into(),
                reason: "base URL cannot-be-a-base".into(),
            })?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

pub fn check_status(
    provider: &str,
    response: &HttpResponse,
    id: &str,
) -> Result<(), ProviderError> {
    match response.status {
        200..=299 => Ok(()),
        404 => Err(ProviderError::NotFound {
            provider: provider.to_string(),
            id: id.to_string(),
        }),
        429 => Err(ProviderError::RateLimited {
            provider: provider.to_string(),
        }),
        other => Err(ProviderError::Request {
            provider: provider.to_string(),
            reason: format!("HTTP {other}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_canned_json() {
        let url = Url::parse("https://example.test/search").unwrap();
        let http = MockHttp::default().get(url.as_str(), r#"{"ok":true}"#);
        let response = http
            .send(HttpRequest {
                method: HttpMethod::Get,
                url: &url,
                headers: &[],
                json_body: None,
            })
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.json("t").unwrap()["ok"], true);
    }

    #[test]
    fn url_with_query_does_not_double_slash() {
        let url = url_with_query("https://openlibrary.org/search.json", &[("q", "dune")]).unwrap();
        assert_eq!(url.host_str(), Some("openlibrary.org"));
        assert!(url.query().unwrap().contains("q=dune"));
    }
}
