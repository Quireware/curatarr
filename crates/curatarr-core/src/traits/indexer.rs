use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::IndexerError;
use crate::types::release::{IndexerQuery, Release};

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
    async fn search(&self, query: &IndexerQuery) -> Result<Vec<Release>, IndexerError>;
    async fn health_check(&self) -> Result<IndexerHealth, IndexerError>;
}
