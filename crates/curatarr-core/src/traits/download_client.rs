use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

#[async_trait]
pub trait DownloadClient: Send + Sync {
    fn name(&self) -> &str;
    fn client_type(&self) -> ClientType;
    async fn health_check(&self) -> Result<ClientHealth, crate::error::CoreError>;
}
