use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{ReadingOrder, SeriesType};
use super::id::{SeriesEntryId, SeriesId, WorkId};
use super::identifiers::ExternalId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub id: SeriesId,
    pub title: String,
    pub sort_title: String,
    pub description: Option<String>,
    pub series_type: SeriesType,
    pub reading_order: ReadingOrder,
    pub volume_count: Option<u32>,
    pub expected_volume_count: Option<u32>,
    pub external_ids: Vec<ExternalId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewSeries {
    pub title: String,
    pub sort_title: String,
    pub description: Option<String>,
    pub series_type: SeriesType,
    pub reading_order: ReadingOrder,
    pub volume_count: Option<u32>,
    pub expected_volume_count: Option<u32>,
    pub external_ids: Vec<ExternalId>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeriesUpdate {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub description: Option<Option<String>>,
    pub series_type: Option<SeriesType>,
    pub reading_order: Option<ReadingOrder>,
    pub volume_count: Option<Option<u32>>,
    pub expected_volume_count: Option<Option<u32>>,
    pub external_ids: Option<Vec<ExternalId>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeriesFilter {
    pub title_contains: Option<String>,
    pub series_type: Option<SeriesType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesEntry {
    pub id: SeriesEntryId,
    pub series_id: SeriesId,
    pub work_id: WorkId,
    pub position: f64,
    pub arc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewSeriesEntry {
    pub series_id: SeriesId,
    pub work_id: WorkId,
    pub position: f64,
    pub arc: Option<String>,
}
