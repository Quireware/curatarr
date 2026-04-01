use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexerProtocol {
    Newznab,
    Torznab,
    Rss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerHealth {
    pub available: bool,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait Indexer: Send + Sync {
    fn name(&self) -> &str;
    fn protocol(&self) -> IndexerProtocol;
    async fn health_check(&self) -> Result<IndexerHealth, crate::error::CoreError>;
}
