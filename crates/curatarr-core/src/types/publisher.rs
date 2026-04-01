use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::id::PublisherId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Publisher {
    pub id: PublisherId,
    pub name: String,
    pub sort_name: String,
    pub imprint: Option<String>,
    pub parent_publisher_id: Option<PublisherId>,
    pub country: Option<String>,
    pub founding_year: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewPublisher {
    pub name: String,
    pub sort_name: String,
    pub imprint: Option<String>,
    pub parent_publisher_id: Option<PublisherId>,
    pub country: Option<String>,
    pub founding_year: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PublisherUpdate {
    pub name: Option<String>,
    pub sort_name: Option<String>,
    pub imprint: Option<Option<String>>,
    pub parent_publisher_id: Option<Option<PublisherId>>,
    pub country: Option<Option<String>>,
    pub founding_year: Option<Option<i32>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PublisherFilter {
    pub name_contains: Option<String>,
    pub country: Option<String>,
}
