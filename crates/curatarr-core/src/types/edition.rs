use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::enums::FileFormat;
use super::id::{EditionId, PublisherId, WorkId};
use super::identifiers::{Asin, Isbn10, Isbn13};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edition {
    pub id: EditionId,
    pub work_id: WorkId,
    pub isbn13: Option<Isbn13>,
    pub isbn10: Option<Isbn10>,
    pub asin: Option<Asin>,
    pub publisher_id: Option<PublisherId>,
    pub imprint: Option<String>,
    pub publication_date: Option<NaiveDate>,
    pub edition_number: Option<u32>,
    pub format: FileFormat,
    pub page_count: Option<u32>,
    pub word_count: Option<u64>,
    pub language: Option<String>,
    pub translator: Option<String>,
    pub cover_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewEdition {
    pub work_id: WorkId,
    pub isbn13: Option<Isbn13>,
    pub isbn10: Option<Isbn10>,
    pub asin: Option<Asin>,
    pub publisher_id: Option<PublisherId>,
    pub imprint: Option<String>,
    pub publication_date: Option<NaiveDate>,
    pub edition_number: Option<u32>,
    pub format: FileFormat,
    pub page_count: Option<u32>,
    pub word_count: Option<u64>,
    pub language: Option<String>,
    pub translator: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EditionUpdate {
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub isbn13: Option<Option<Isbn13>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub isbn10: Option<Option<Isbn10>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub asin: Option<Option<Asin>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub publisher_id: Option<Option<PublisherId>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub imprint: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub publication_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub edition_number: Option<Option<u32>>,
    pub format: Option<FileFormat>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub page_count: Option<Option<u32>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub word_count: Option<Option<u64>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub language: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub translator: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub cover_path: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EditionFilter {
    pub work_id: Option<WorkId>,
    pub format: Option<FileFormat>,
    pub language: Option<String>,
}
