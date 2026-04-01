use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::id::TagId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub parent_id: Option<TagId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewTag {
    pub name: String,
    pub parent_id: Option<TagId>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TagUpdate {
    pub name: Option<String>,
    pub parent_id: Option<Option<TagId>>,
}
