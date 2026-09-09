use async_trait::async_trait;
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{MetadataMatch, MetadataQuery, WorkMetadata};

const NAME: &str = "goodreads";
const REASON: &str = "Goodreads public API was shut down; scraping is out of scope";

pub struct Goodreads;

fn disabled() -> ProviderError {
    ProviderError::Disabled {
        provider: NAME.into(),
        reason: REASON.into(),
    }
}

#[async_trait]
impl MetadataProvider for Goodreads {
    fn name(&self) -> &str {
        NAME
    }
    fn supported_content_types(&self) -> &[ContentType] {
        &[]
    }

    async fn search(&self, _query: &MetadataQuery) -> Result<Vec<MetadataMatch>, ProviderError> {
        Err(disabled())
    }

    async fn fetch_work(&self, _id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
        Err(disabled())
    }

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
        Ok(ProviderHealth {
            available: false,
            latency_ms: None,
            last_error: Some(REASON.into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_is_disabled() {
        let err = Goodreads
            .search(&MetadataQuery::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("disabled"));
    }
}
