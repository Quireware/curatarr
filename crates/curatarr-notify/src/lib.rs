use async_trait::async_trait;
use curatarr_core::error::CoreError;
use curatarr_core::traits::notifier::{Notification, Notifier};
use curatarr_metadata::http::{HttpClient, HttpMethod, HttpRequest, url_join};
use std::sync::Arc;
use url::Url;

pub struct Ntfy {
    http: Arc<dyn HttpClient>,
    base: Url,
    topic: String,
}

impl Ntfy {
    pub fn new(http: Arc<dyn HttpClient>, base: Url, topic: impl Into<String>) -> Self {
        Self {
            http,
            base,
            topic: topic.into(),
        }
    }
}

#[async_trait]
impl Notifier for Ntfy {
    fn name(&self) -> &str {
        "ntfy"
    }

    async fn notify(&self, event: &Notification) -> Result<(), CoreError> {
        self.send(&event.title, &event.body).await
    }

    async fn test(&self) -> Result<(), CoreError> {
        self.send("curatarr", "test").await
    }
}

impl Ntfy {
    pub async fn send(&self, title: &str, message: &str) -> Result<(), CoreError> {
        if self.topic.is_empty() {
            return Ok(());
        }
        let url =
            url_join(self.base.as_str(), &[&self.topic]).map_err(|e| CoreError::Validation {
                field: "ntfy".into(),
                reason: e.to_string(),
            })?;
        let headers = [("title", title), ("message", message)];
        self.http
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: &url,
                headers: &headers,
                json_body: None,
                form_body: None,
            })
            .await
            .map_err(|e| CoreError::Validation {
                field: "ntfy".into(),
                reason: e.to_string(),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_metadata::http::MockHttp;

    #[tokio::test]
    async fn empty_topic_is_noop() {
        let http = Arc::new(MockHttp::default());
        let ntfy = Ntfy::new(http, Url::parse("https://ntfy.sh/").unwrap(), "");
        ntfy.send("t", "m").await.unwrap();
    }

    #[tokio::test]
    async fn posts_to_topic() {
        let url = Url::parse("https://ntfy.sh/curatarr").unwrap();
        let http = Arc::new(MockHttp::default().post(url.as_str(), "ok"));
        let ntfy = Ntfy::new(
            http.clone(),
            Url::parse("https://ntfy.sh/").unwrap(),
            "curatarr",
        );
        ntfy.send("grabbed", "Dune").await.unwrap();
        let last = http.last().unwrap();
        assert_eq!(last.url, "https://ntfy.sh/curatarr");
        assert!(
            last.headers
                .iter()
                .any(|(k, v)| k == "title" && v == "grabbed")
        );
    }
}
