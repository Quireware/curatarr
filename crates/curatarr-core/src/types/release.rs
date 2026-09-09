use serde::{Deserialize, Serialize};
use url::Url;

use super::enums::ContentType;
use crate::traits::indexer::IndexerProtocol;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexerQuery {
    pub query: String,
    pub content_type: Option<ContentType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub title: String,
    pub guid: String,
    pub indexer: String,
    pub download_url: Url,
    pub size_bytes: u64,
    pub protocol: IndexerProtocol,
    #[serde(default)]
    pub info_hash: Option<String>,
}
