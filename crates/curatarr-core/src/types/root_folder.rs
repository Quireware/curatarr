use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::ContentType;
use super::id::RootFolderId;

/// A managed library directory. Files under a root folder are scanned in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RootFolder {
    pub id: RootFolderId,
    pub path: String,
    pub name: Option<String>,
    pub content_types: Vec<ContentType>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewRootFolder {
    pub path: String,
    pub name: Option<String>,
    #[serde(default)]
    pub content_types: Vec<ContentType>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_folder_serde_roundtrip() {
        let now = Utc::now();
        let folder = RootFolder {
            id: RootFolderId::new(),
            path: "/books".into(),
            name: Some("Main".into()),
            content_types: vec![ContentType::Book, ContentType::Manga],
            created_at: now,
        };
        let json = serde_json::to_string(&folder).unwrap();
        let back: RootFolder = serde_json::from_str(&json).unwrap();
        assert_eq!(folder, back);
    }

    #[test]
    fn new_root_folder_content_types_default_empty() {
        let json = r#"{"path":"/books","name":null}"#;
        let parsed: NewRootFolder = serde_json::from_str(json).unwrap();
        assert!(parsed.content_types.is_empty());
    }
}
