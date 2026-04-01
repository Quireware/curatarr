use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::id::AuthorId;
use super::identifiers::ExternalId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub id: AuthorId,
    pub name: String,
    pub sort_name: String,
    pub birth_date: Option<NaiveDate>,
    pub death_date: Option<NaiveDate>,
    pub nationality: Option<String>,
    pub biography: Option<String>,
    pub biography_html: Option<String>,
    pub photo_path: Option<String>,
    pub external_ids: Vec<ExternalId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewAuthor {
    pub name: String,
    pub sort_name: String,
    pub birth_date: Option<NaiveDate>,
    pub death_date: Option<NaiveDate>,
    pub nationality: Option<String>,
    pub biography: Option<String>,
    pub biography_html: Option<String>,
    pub external_ids: Vec<ExternalId>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorUpdate {
    pub name: Option<String>,
    pub sort_name: Option<String>,
    pub birth_date: Option<Option<NaiveDate>>,
    pub death_date: Option<Option<NaiveDate>>,
    pub nationality: Option<Option<String>>,
    pub biography: Option<Option<String>>,
    pub biography_html: Option<Option<String>>,
    pub photo_path: Option<Option<String>>,
    pub external_ids: Option<Vec<ExternalId>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorFilter {
    pub name_contains: Option<String>,
    pub nationality: Option<String>,
}
