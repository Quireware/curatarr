use async_trait::async_trait;
use base64::Engine;
use curatarr_core::error::ClientError;
use curatarr_core::traits::download_client::{
    ClientHealth, ClientType, DownloadClient, DownloadRequest, DownloadStatus,
};
use curatarr_core::types::enums::DownloadState;
use curatarr_metadata::http::{HttpClient, HttpMethod, HttpRequest, check_status};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

pub struct Nzbget {
    http: Arc<dyn HttpClient>,
    rpc: Url,
    username: String,
    password: String,
    dest_dir: PathBuf,
}

impl Nzbget {
    pub fn new(
        http: Arc<dyn HttpClient>,
        rpc: Url,
        username: impl Into<String>,
        password: impl Into<String>,
        dest_dir: PathBuf,
    ) -> Self {
        Self {
            http,
            rpc,
            username: username.into(),
            password: password.into(),
            dest_dir,
        }
    }

    fn auth_header(&self) -> String {
        let raw = format!("{}:{}", self.username, self.password);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
        )
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let auth = self.auth_header();
        let body = json!({ "method": method, "params": params });
        let headers = [
            ("authorization", auth.as_str()),
            ("content-type", "application/json"),
        ];
        let response = self
            .http
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: &self.rpc,
                headers: &headers,
                json_body: Some(&body),
                form_body: None,
            })
            .await?;
        check_status("nzbget", &response, method)?;
        Ok(response.json("nzbget")?)
    }
}

#[async_trait]
impl DownloadClient for Nzbget {
    fn name(&self) -> &str {
        "nzbget"
    }

    fn client_type(&self) -> ClientType {
        ClientType::Usenet
    }

    async fn add_download(&self, request: &DownloadRequest) -> Result<String, ClientError> {
        let category = request.category.clone().unwrap_or_default();
        let params = json!([
            request.name,
            request.url.as_str(),
            category,
            0,
            false,
            false,
            request.idempotency_key,
            0,
            "SCORE"
        ]);
        let body = self.rpc("append", params).await?;
        let nzb_id = body
            .get("result")
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
            .unwrap_or_else(|| request.idempotency_key.clone()); // clone: fallback client_ref
        Ok(nzb_id)
    }

    async fn get_status(&self, client_ref: &str) -> Result<DownloadStatus, ClientError> {
        let list = self.list_downloads().await?;
        list.into_iter()
            .find(|s| s.client_ref == client_ref)
            .ok_or_else(|| {
                curatarr_core::error::ProviderError::NotFound {
                    provider: "nzbget".into(),
                    id: client_ref.to_string(),
                }
                .into()
            })
    }

    async fn list_downloads(&self) -> Result<Vec<DownloadStatus>, ClientError> {
        let history = self.rpc("history", json!([false])).await?;
        let items = history
            .pointer("/result")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|item| history_item(item, &self.dest_dir))
            .collect())
    }

    async fn health_check(&self) -> Result<ClientHealth, ClientError> {
        match self.rpc("version", json!([])).await {
            Ok(_) => Ok(ClientHealth {
                available: true,
                version: None,
                last_error: None,
            }),
            Err(e) => Ok(ClientHealth {
                available: false,
                version: None,
                last_error: Some(e.to_string()),
            }),
        }
    }
}

fn history_item(item: &Value, dest_dir: &std::path::Path) -> Option<DownloadStatus> {
    let id = item
        .get("NZBID")
        .and_then(Value::as_i64)
        .map(|n| n.to_string())
        .or_else(|| item.get("Name").and_then(Value::as_str).map(str::to_string))?;
    let status = item.get("Status").and_then(Value::as_str).unwrap_or("");
    let name = item.get("Name").and_then(Value::as_str).unwrap_or("");
    let (state, error) = match status {
        "SUCCESS" | "SUCCESS/UNPACK" | "SUCCESS/HEALTH" => (DownloadState::Imported, None),
        s if s.starts_with("FAILURE") || s.starts_with("DELETED") => {
            (DownloadState::Failed, Some(s.to_string()))
        }
        _ => (DownloadState::Downloading, None),
    };
    let output_path = if state == DownloadState::Imported {
        Some(dest_dir.join(name))
    } else {
        None
    };
    Some(DownloadStatus {
        client_ref: id,
        name: if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        },
        state,
        output_path,
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_metadata::http::MockHttp;

    #[tokio::test]
    async fn append_sends_basic_auth_and_jsonrpc_method() {
        let rpc = Url::parse("http://nzbget.test/jsonrpc").unwrap();
        let body = r#"{"result": 42}"#;
        let http = Arc::new(MockHttp::default().post(rpc.as_str(), body));
        let client = Nzbget::new(
            http.clone(),
            rpc,
            "nzbget",
            "tegbzn6789",
            PathBuf::from("/downloads/complete"),
        );
        let id = client
            .add_download(&DownloadRequest {
                name: "Dune".into(),
                url: Url::parse("http://idx/d.nzb").unwrap(),
                category: None,
                idempotency_key: "q-1".into(),
                info_hash: None,
            })
            .await
            .unwrap();
        assert_eq!(id, "42");
        let last = http.last().unwrap();
        assert!(
            last.headers
                .iter()
                .any(|(k, v)| k == "authorization" && v.starts_with("Basic "))
        );
        assert_eq!(last.body.unwrap()["method"], "append");
    }

    #[tokio::test]
    async fn history_success_maps_completed_path() {
        let rpc = Url::parse("http://nzbget.test/jsonrpc").unwrap();
        let body = r#"{"result":[{"NZBID":7,"Name":"Dune","Status":"SUCCESS"}]}"#;
        let http = Arc::new(MockHttp::default().post(rpc.as_str(), body));
        let client = Nzbget::new(http, rpc, "u", "p", PathBuf::from("/downloads/complete"));
        let status = client.get_status("7").await.unwrap();
        assert_eq!(status.state, DownloadState::Imported);
        assert_eq!(
            status.output_path.as_deref(),
            Some(std::path::Path::new("/downloads/complete/Dune"))
        );
    }
}
