mod apply;
pub mod enrich;
pub mod http;
pub mod merge;
pub mod providers;
pub mod rate_limit;
pub mod registry;
pub mod score;

pub use enrich::{EnrichError, EnrichmentService};
pub use http::{HttpClient, MockHttp, ReqwestHttp};
pub use registry::ProviderRegistry;

use curatarr_config::metadata::MetadataConfig;
use std::sync::Arc;

pub fn registry_from_config(
    config: &MetadataConfig,
    http: Arc<dyn HttpClient>,
) -> ProviderRegistry {
    ProviderRegistry::from_config(config, http)
}
