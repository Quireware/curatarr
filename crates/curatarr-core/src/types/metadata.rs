use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use super::enums::{AgeRating, ContentType};
use super::id::WorkId;
use super::identifiers::ExternalId;

pub mod fields {
    pub const TITLE: &str = "title";
    pub const SORT_TITLE: &str = "sort_title";
    pub const ORIGINAL_LANGUAGE: &str = "original_language";
    pub const ORIGINAL_PUB_DATE: &str = "original_pub_date";
    pub const DESCRIPTION: &str = "description";
    pub const AGE_RATING: &str = "age_rating";
    pub const AVERAGE_RATING: &str = "average_rating";
    pub const AUTHORS: &str = "authors";
    pub const SERIES: &str = "series";
    pub const TAGS: &str = "tags";
    pub const ISBN13: &str = "isbn13";
    pub const PAGE_COUNT: &str = "page_count";
    pub const COVER: &str = "cover";

    pub const ALL: &[&str] = &[
        TITLE,
        SORT_TITLE,
        ORIGINAL_LANGUAGE,
        ORIGINAL_PUB_DATE,
        DESCRIPTION,
        AGE_RATING,
        AVERAGE_RATING,
        AUTHORS,
        SERIES,
        TAGS,
        ISBN13,
        PAGE_COUNT,
        COVER,
    ];

    pub fn is_known(field: &str) -> bool {
        ALL.contains(&field)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Work,
    Edition,
    Author,
    Series,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Edition => "edition",
            Self::Author => "author",
            Self::Series => "series",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "work" => Some(Self::Work),
            "edition" => Some(Self::Edition),
            "author" => Some(Self::Author),
            "series" => Some(Self::Series),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataQuery {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    pub year: Option<i32>,
    pub content_type: Option<ContentType>,
    pub series: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataMatch {
    pub provider: String,
    pub external_id: ExternalId,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub description: Option<String>,
    pub cover_url: Option<Url>,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorRef {
    pub name: String,
    pub external_id: Option<ExternalId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesRef {
    pub title: String,
    pub position: Option<f64>,
    pub external_id: Option<ExternalId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkMetadata {
    pub provider: String,
    pub external_id: ExternalId,
    pub title: String,
    pub sort_title: Option<String>,
    pub original_language: Option<String>,
    pub original_pub_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub content_type: ContentType,
    pub age_rating: Option<AgeRating>,
    pub average_rating: Option<f64>,
    pub authors: Vec<AuthorRef>,
    pub series: Option<SeriesRef>,
    pub tags: Vec<String>,
    pub identifiers: Vec<ExternalId>,
    pub isbn13: Option<String>,
    pub isbn10: Option<String>,
    pub page_count: Option<u32>,
    pub cover_url: Option<Url>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorMetadata {
    pub provider: String,
    pub external_id: ExternalId,
    pub name: String,
    pub biography: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub death_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesMetadata {
    pub provider: String,
    pub external_id: ExternalId,
    pub title: String,
    pub description: Option<String>,
    pub volume_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditionMetadata {
    pub provider: String,
    pub external_id: ExternalId,
    pub isbn13: Option<String>,
    pub isbn10: Option<String>,
    pub page_count: Option<u32>,
    pub language: Option<String>,
    pub publication_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverImage {
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
    pub source_url: Option<Url>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataSnapshot {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_language: Option<String>,
    pub original_pub_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub age_rating: Option<AgeRating>,
    pub average_rating: Option<f64>,
    pub authors: Vec<String>,
    pub series_title: Option<String>,
    pub series_position: Option<f64>,
    pub tags: Vec<String>,
    pub isbn13: Option<String>,
    pub page_count: Option<u32>,
    pub cover_url: Option<Url>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldLock {
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub field: String,
    pub locked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSource {
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub field: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewAuditEvent {
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldConflict {
    pub field: String,
    pub values: Vec<ConflictValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictValue {
    pub source: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeResult {
    pub snapshot: MetadataSnapshot,
    pub applied: Vec<FieldChange>,
    pub skipped_locked: Vec<String>,
    pub conflicts: Vec<FieldConflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshReport {
    pub work_id: WorkId,
    pub matches: Vec<MetadataMatch>,
    pub applied: Vec<FieldChange>,
    pub skipped_locked: Vec<String>,
    pub conflicts: Vec<FieldConflict>,
    pub provider_errors: Vec<String>,
    pub preview: bool,
}

impl From<&WorkMetadata> for MetadataSnapshot {
    fn from(meta: &WorkMetadata) -> Self {
        Self {
            title: nonempty(&meta.title),
            sort_title: meta.sort_title.clone(), // clone: snapshot is an owned merge document
            original_language: meta.original_language.clone(), // clone: snapshot is an owned merge document
            original_pub_date: meta.original_pub_date,
            description: meta.description.clone(), // clone: snapshot is an owned merge document
            age_rating: meta.age_rating,
            average_rating: meta.average_rating,
            authors: meta
                .authors
                .iter()
                .map(|a| a.name.clone()) // clone: snapshot is an owned merge document
                .collect(),
            series_title: meta.series.as_ref().map(|s| s.title.clone()), // clone: snapshot is an owned merge document
            series_position: meta.series.as_ref().and_then(|s| s.position),
            tags: meta.tags.clone(), // clone: snapshot is an owned merge document
            isbn13: meta.isbn13.clone(), // clone: snapshot is an owned merge document
            page_count: meta.page_count,
            cover_url: meta.cover_url.clone(), // clone: Url is not Copy
        }
    }
}

fn nonempty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::identifiers::ExternalId;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn query_serde_roundtrip(title in "[a-zA-Z0-9 ]{0,40}") {
            let q = MetadataQuery {
                title: nonempty(&title),
                authors: vec!["Herbert".into()],
                isbn: None,
                year: Some(1965),
                content_type: Some(ContentType::Book),
                series: None,
            };
            let json = serde_json::to_string(&q).unwrap();
            let back: MetadataQuery = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(q, back);
        }
    }

    #[test]
    fn known_fields_include_title() {
        assert!(fields::is_known("title"));
        assert!(!fields::is_known("user_notes"));
    }

    #[test]
    fn entity_kind_roundtrip() {
        for kind in [
            EntityKind::Work,
            EntityKind::Edition,
            EntityKind::Author,
            EntityKind::Series,
        ] {
            assert_eq!(EntityKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EntityKind::parse("nope"), None);
    }

    #[test]
    fn snapshot_from_work_metadata_skips_blank_title() {
        let meta = WorkMetadata {
            provider: "openlibrary".into(),
            external_id: ExternalId::OpenLibraryWork("OL1W".into()),
            title: "  ".into(),
            sort_title: None,
            original_language: None,
            original_pub_date: None,
            description: None,
            content_type: ContentType::Book,
            age_rating: None,
            average_rating: None,
            authors: vec![],
            series: None,
            tags: vec![],
            identifiers: vec![],
            isbn13: None,
            isbn10: None,
            page_count: None,
            cover_url: None,
        };
        let snap = MetadataSnapshot::from(&meta);
        assert!(snap.title.is_none());
    }
}
