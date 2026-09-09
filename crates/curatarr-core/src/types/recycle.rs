use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::id::{FileId, RecycleEntryId};

/// A soft-deleted library file parked in the recycle bin awaiting restore or purge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecycleEntry {
    pub id: RecycleEntryId,
    pub original_file_id: FileId,
    pub original_path: String,
    pub recycle_path: String,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewRecycleEntry {
    pub original_file_id: FileId,
    pub original_path: String,
    pub recycle_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recycle_entry_serde_roundtrip() {
        let entry = RecycleEntry {
            id: RecycleEntryId::new(),
            original_file_id: FileId::new(),
            original_path: "/books/a.epub".into(),
            recycle_path: "/data/recycle/x/a.epub".into(),
            deleted_at: Utc::now(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: RecycleEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }
}
