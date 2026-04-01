use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::enums::ContentType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub available: bool,
    pub latency_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_content_types(&self) -> &[ContentType];
    async fn health_check(&self) -> Result<ProviderHealth, crate::error::CoreError>;
}
