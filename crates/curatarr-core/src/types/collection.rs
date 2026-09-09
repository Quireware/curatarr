use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::id::CollectionId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewCollection {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionUpdate {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "super::serde_helpers::double_option")]
    pub description: Option<Option<String>>,
}
