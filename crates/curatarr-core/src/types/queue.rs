use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::DownloadState;
use super::id::{QueueItemId, WorkId};
use crate::traits::indexer::IndexerProtocol;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: QueueItemId,
    pub work_id: WorkId,
    pub state: DownloadState,
    pub protocol: Option<IndexerProtocol>,
    pub indexer: Option<String>,
    pub title: String,
    pub guid: Option<String>,
    pub download_url: Option<String>,
    pub client: Option<String>,
    pub client_ref: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub idempotency_key: String,
    pub revision: i64,
    pub grabbed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewQueueItem {
    pub work_id: WorkId,
    pub state: DownloadState,
    pub protocol: Option<IndexerProtocol>,
    pub indexer: Option<String>,
    pub title: String,
    pub guid: Option<String>,
    pub download_url: Option<String>,
    pub client: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueueItemUpdate {
    pub state: Option<DownloadState>,
    pub client_ref: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}
