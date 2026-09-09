use async_trait::async_trait;
use curatarr_core::error::ClientError;
use curatarr_core::traits::download_client::{
    ClientHealth, ClientType, DownloadClient, DownloadRequest, DownloadStatus,
};
use curatarr_core::types::enums::DownloadState;
use curatarr_metadata::http::{
    HttpClient, HttpMethod, HttpRequest, HttpResponse, check_status, url_join, url_with_query,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use url::Url;

pub struct Qbittorrent {
    http: Arc<dyn HttpClient>,
    base: Url,
    username: String,
    password: String,
    save_path: PathBuf,
    sid: Mutex<Option<String>>,
}

impl Qbittorrent {
    pub fn new(
        http: Arc<dyn HttpClient>,
        base: Url,
        username: impl Into<String>,
        password: impl Into<String>,
        save_path: PathBuf,
    ) -> Self {
        Self {
            http,
            base,
            username: username.into(),
            password: password.into(),
            save_path,
            sid: Mutex::new(None),
        }
    }

    fn login_url(&self) -> Result<Url, ClientError> {
        Ok(url_join(
            self.base.as_str(),
            &["api", "v2", "auth", "login"],
        )?)
    }

    fn add_url(&self) -> Result<Url, ClientError> {
        Ok(url_join(
            self.base.as_str(),
            &["api", "v2", "torrents", "add"],
        )?)
    }

    fn info_url(&self, hash: Option<&str>) -> Result<Url, ClientError> {
        let base = url_join(self.base.as_str(), &["api", "v2", "torrents", "info"])?;
        match hash {
            Some(h) => Ok(url_with_query(base.as_str(), &[("hashes", h)])?),
            None => Ok(base),
        }
    }

    async fn login(&self) -> Result<String, ClientError> {
        let url = self.login_url()?;
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("username", &self.username)
            .append_pair("password", &self.password)
            .finish();
        let response = self
            .http
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: &url,
                headers: &[],
                json_body: None,
                form_body: Some(&form),
            })
            .await?;
        check_status("qbittorrent", &response, "login")?;
        let sid = sid_from_headers(&response.headers).ok_or_else(|| {
            curatarr_core::error::ProviderError::Parse {
                provider: "qbittorrent".into(),
                reason: "login response missing SID cookie".into(),
            }
        })?;
        *self.sid.lock().unwrap_or_else(|e| e.into_inner()) = Some(sid.clone()); // clone: stored and returned
        Ok(sid)
    }

    async fn cookie(&self) -> Result<String, ClientError> {
        let cached = self.sid.lock().unwrap_or_else(|e| e.into_inner()).clone(); // clone: header owns a copy of SID
        if let Some(sid) = cached {
            return Ok(cookie_header(&sid));
        }
        let sid = self.login().await?;
        Ok(cookie_header(&sid))
    }

    async fn send_authed(
        &self,
        method: HttpMethod,
        url: &Url,
        form_body: Option<&str>,
    ) -> Result<HttpResponse, ClientError> {
        let cookie = self.cookie().await?;
        let headers = [("cookie", cookie.as_str())];
        Ok(self
            .http
            .send(HttpRequest {
                method,
                url,
                headers: &headers,
                json_body: None,
                form_body,
            })
            .await?)
    }
}

#[async_trait]
impl DownloadClient for Qbittorrent {
    fn name(&self) -> &str {
        "qbittorrent"
    }

    fn client_type(&self) -> ClientType {
        ClientType::Torrent
    }

    async fn add_download(&self, request: &DownloadRequest) -> Result<String, ClientError> {
        let hash =
            info_hash(request).ok_or_else(|| curatarr_core::error::ProviderError::Request {
                provider: "qbittorrent".into(),
                reason: "torrent info hash missing".into(),
            })?;
        let url = self.add_url()?;
        let body = encode_add_form(request.url.as_str(), &self.save_path);
        let response = self
            .send_authed(HttpMethod::Post, &url, Some(&body))
            .await?;
        check_status("qbittorrent", &response, "torrents/add")?;
        Ok(hash)
    }

    async fn get_status(&self, client_ref: &str) -> Result<DownloadStatus, ClientError> {
        let url = self.info_url(Some(client_ref))?;
        let response = self.send_authed(HttpMethod::Get, &url, None).await?;
        check_status("qbittorrent", &response, client_ref)?;
        let body = response.json("qbittorrent")?;
        parse_torrents(&body)
            .into_iter()
            .find(|s| s.client_ref == client_ref)
            .ok_or_else(|| {
                curatarr_core::error::ProviderError::NotFound {
                    provider: "qbittorrent".into(),
                    id: client_ref.to_string(),
                }
                .into()
            })
    }

    async fn list_downloads(&self) -> Result<Vec<DownloadStatus>, ClientError> {
        let url = self.info_url(None)?;
        let response = self.send_authed(HttpMethod::Get, &url, None).await?;
        check_status("qbittorrent", &response, "torrents/info")?;
        let body = response.json("qbittorrent")?;
        Ok(parse_torrents(&body))
    }

    async fn health_check(&self) -> Result<ClientHealth, ClientError> {
        let url = url_join(self.base.as_str(), &["api", "v2", "app", "version"])?;
        match self.send_authed(HttpMethod::Get, &url, None).await {
            Ok(resp) if (200..300).contains(&resp.status) => Ok(ClientHealth {
                available: true,
                version: String::from_utf8(resp.body).ok(),
                last_error: None,
            }),
            Ok(resp) => Ok(ClientHealth {
                available: false,
                version: None,
                last_error: Some(format!("HTTP {}", resp.status)),
            }),
            Err(e) => Ok(ClientHealth {
                available: false,
                version: None,
                last_error: Some(e.to_string()),
            }),
        }
    }
}

fn encode_add_form(urls: &str, save_path: &std::path::Path) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("urls", urls);
    if !save_path.as_os_str().is_empty() {
        form.append_pair("savepath", &save_path.to_string_lossy());
    }
    form.finish()
}

fn cookie_header(sid: &str) -> String {
    let mut value = String::from("SID=");
    value.push_str(sid);
    value
}

fn sid_from_headers(headers: &[(String, String)]) -> Option<String> {
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        for part in value.split(';') {
            let part = part.trim();
            if let Some(sid) = part
                .strip_prefix("SID=")
                .or_else(|| part.strip_prefix("sid="))
            {
                if !sid.is_empty() {
                    return Some(sid.to_string());
                }
            }
        }
    }
    None
}

fn info_hash(request: &DownloadRequest) -> Option<String> {
    if let Some(hash) = &request.info_hash {
        if !hash.is_empty() {
            return Some(hash.clone()); // clone: client_ref owns the hash
        }
    }
    btih(&request.url)
}

fn btih(url: &Url) -> Option<String> {
    if url.scheme() != "magnet" {
        return None;
    }
    for (key, value) in url.query_pairs() {
        if key != "xt" {
            continue;
        }
        let lower = value.to_ascii_lowercase();
        if let Some(hash) = lower.strip_prefix("urn:btih:") {
            return Some(hash.to_string());
        }
    }
    None
}

fn parse_torrents(body: &Value) -> Vec<DownloadStatus> {
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_torrent)
        .collect()
}

fn parse_torrent(item: &Value) -> Option<DownloadStatus> {
    let hash = item.get("hash")?.as_str()?.to_string();
    let state = item.get("state").and_then(Value::as_str).unwrap_or("");
    let name = item.get("name").and_then(Value::as_str).map(str::to_string);
    let path = item
        .get("content_path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| item.get("save_path").and_then(Value::as_str));
    let mapped = map_state(state);
    Some(DownloadStatus {
        client_ref: hash,
        name,
        state: mapped,
        output_path: path.map(PathBuf::from),
        error: if mapped == DownloadState::Failed {
            Some(state.to_string())
        } else {
            None
        },
    })
}

fn map_state(state: &str) -> DownloadState {
    match state {
        "error" | "missingFiles" | "unknown" => DownloadState::Failed,
        "uploading" | "stalledUP" | "pausedUP" | "queuedUP" | "checkingUP" | "forcedUP"
        | "stoppedUP" => DownloadState::Imported,
        _ => DownloadState::Downloading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_metadata::http::MockHttp;

    fn client(http: Arc<MockHttp>, base: Url) -> Qbittorrent {
        Qbittorrent::new(http, base, "admin", "secret", PathBuf::new())
    }

    #[tokio::test]
    async fn add_logs_in_and_posts_urls() {
        let base = Url::parse("http://qbit.test/").unwrap();
        let login = url_join(base.as_str(), &["api", "v2", "auth", "login"]).unwrap();
        let add = url_join(base.as_str(), &["api", "v2", "torrents", "add"]).unwrap();
        let http = Arc::new(
            MockHttp::default()
                .post_with_headers(
                    login.as_str(),
                    "Ok.",
                    &[("set-cookie", "SID=abc; HttpOnly")],
                )
                .post(add.as_str(), "Ok."),
        );
        let qbit = client(http.clone(), base);
        let hash = qbit
            .add_download(&DownloadRequest {
                name: "Dune".into(),
                url: Url::parse("magnet:?xt=urn:btih:deadbeef").unwrap(),
                category: None,
                idempotency_key: "q-1".into(),
                info_hash: Some("deadbeef".into()),
            })
            .await
            .unwrap();
        assert_eq!(hash, "deadbeef");
        let rec = http.recorded();
        assert_eq!(rec.len(), 2);
        assert!(
            rec[0]
                .form
                .as_deref()
                .is_some_and(|f| f.contains("username=admin"))
        );
        assert!(
            rec[1]
                .headers
                .iter()
                .any(|(k, v)| k == "cookie" && v == "SID=abc")
        );
        assert!(
            rec[1]
                .form
                .as_deref()
                .is_some_and(|f| f.contains("urls=magnet"))
        );
    }

    #[tokio::test]
    async fn info_hash_status_maps_content_path() {
        let base = Url::parse("http://qbit.test/").unwrap();
        let login = url_join(base.as_str(), &["api", "v2", "auth", "login"]).unwrap();
        let info = url_with_query(
            url_join(base.as_str(), &["api", "v2", "torrents", "info"])
                .unwrap()
                .as_str(),
            &[("hashes", "deadbeef")],
        )
        .unwrap();
        let body = r#"[{"hash":"deadbeef","name":"Dune","state":"uploading","content_path":"/downloads/Dune"}]"#;
        let http = Arc::new(
            MockHttp::default()
                .post_with_headers(login.as_str(), "Ok.", &[("set-cookie", "SID=abc")])
                .get(info.as_str(), body),
        );
        let status = client(http, base).get_status("deadbeef").await.unwrap();
        assert_eq!(status.state, DownloadState::Imported);
        assert_eq!(
            status.output_path.as_deref(),
            Some(std::path::Path::new("/downloads/Dune"))
        );
    }
}
