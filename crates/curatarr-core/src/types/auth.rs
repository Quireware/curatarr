use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::id::{SessionId, UserId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub user_id: UserId,
    pub csrf_token: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginAttempt {
    pub attempt_key: String,
    pub failures: i64,
    pub locked_until: Option<DateTime<Utc>>,
}
