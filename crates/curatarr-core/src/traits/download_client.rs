use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::ClientError;
use crate::types::enums::DownloadState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientType {
    Usenet,
    Torrent,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHealth {
    pub available: bool,
    pub version: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub name: String,
    pub url: url::Url,
    pub category: Option<String>,
    pub idempotency_key: String,
    pub info_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadStatus {
    pub client_ref: String,
    pub name: Option<String>,
    pub state: DownloadState,
    pub output_path: Option<PathBuf>,
    pub error: Option<String>,
}

#[async_trait]
pub trait DownloadClient: Send + Sync {
    fn name(&self) -> &str;
    fn client_type(&self) -> ClientType;
    async fn add_download(&self, request: &DownloadRequest) -> Result<String, ClientError>;
    async fn get_status(&self, client_ref: &str) -> Result<DownloadStatus, ClientError>;
    async fn list_downloads(&self) -> Result<Vec<DownloadStatus>, ClientError>;
    async fn health_check(&self) -> Result<ClientHealth, ClientError>;
}
