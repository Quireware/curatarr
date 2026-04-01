use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{AgeRating, ContentType, ReadStatus};
use super::id::WorkId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Work {
    pub id: WorkId,
    pub title: String,
    pub sort_title: String,
    pub original_language: Option<String>,
    pub original_pub_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub description_html: Option<String>,
    pub content_type: ContentType,
    pub age_rating: Option<AgeRating>,
    pub content_warnings: Vec<String>,
    pub average_rating: Option<f64>,
    pub user_rating: Option<f64>,
    pub user_review: Option<String>,
    pub read_status: ReadStatus,
    pub user_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewWork {
    pub title: String,
    pub sort_title: String,
    pub original_language: Option<String>,
    pub original_pub_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub description_html: Option<String>,
    pub content_type: ContentType,
    pub age_rating: Option<AgeRating>,
    pub content_warnings: Vec<String>,
    pub read_status: ReadStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkUpdate {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_language: Option<Option<String>>,
    pub original_pub_date: Option<Option<NaiveDate>>,
    pub description: Option<Option<String>>,
    pub description_html: Option<Option<String>>,
    pub content_type: Option<ContentType>,
    pub age_rating: Option<Option<AgeRating>>,
    pub content_warnings: Option<Vec<String>>,
    pub average_rating: Option<Option<f64>>,
    pub user_rating: Option<Option<f64>>,
    pub user_review: Option<Option<String>>,
    pub read_status: Option<ReadStatus>,
    pub user_notes: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkFilter {
    pub content_type: Option<ContentType>,
    pub read_status: Option<ReadStatus>,
    pub age_rating: Option<AgeRating>,
    pub title_contains: Option<String>,
    pub language: Option<String>,
}
