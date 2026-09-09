use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ProviderError;
use crate::types::enums::ContentType;
use crate::types::identifiers::ExternalId;
use crate::types::metadata::{
    AuthorMetadata, CoverImage, EditionMetadata, MetadataMatch, MetadataQuery, SeriesMetadata,
    WorkMetadata,
};

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

    async fn search(&self, query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError>;
    async fn fetch_work(&self, id: &ExternalId) -> Result<WorkMetadata, ProviderError>;
    async fn health_check(&self) -> Result<ProviderHealth, ProviderError>;

    async fn fetch_edition(&self, id: &ExternalId) -> Result<EditionMetadata, ProviderError> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: format!("fetch_edition:{id:?}"),
        })
    }

    async fn fetch_author(&self, id: &ExternalId) -> Result<AuthorMetadata, ProviderError> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: format!("fetch_author:{id:?}"),
        })
    }

    async fn fetch_series(&self, id: &ExternalId) -> Result<SeriesMetadata, ProviderError> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: format!("fetch_series:{id:?}"),
        })
    }

    async fn fetch_cover(&self, id: &ExternalId) -> Result<CoverImage, ProviderError> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: format!("fetch_cover:{id:?}"),
        })
    }

    async fn author_works(&self, id: &ExternalId) -> Result<Vec<MetadataMatch>, ProviderError> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: format!("author_works:{id:?}"),
        })
    }

    async fn series_works(&self, id: &ExternalId) -> Result<Vec<MetadataMatch>, ProviderError> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: format!("series_works:{id:?}"),
        })
    }
}
