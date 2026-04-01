use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::FileFormat;
use super::id::{EditionId, FileId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryFile {
    pub id: FileId,
    pub edition_id: EditionId,
    pub path: String,
    pub format: FileFormat,
    pub size_bytes: u64,
    pub sha256: String,
    pub import_date: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewLibraryFile {
    pub edition_id: EditionId,
    pub path: String,
    pub format: FileFormat,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryFileUpdate {
    pub path: Option<String>,
    pub deleted_at: Option<Option<DateTime<Utc>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FileFilter {
    pub edition_id: Option<EditionId>,
    pub format: Option<FileFormat>,
    pub include_deleted: bool,
}
