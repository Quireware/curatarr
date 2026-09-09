use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::sqlite::{now_iso, parse_dt, parse_opt_dt, parse_uuid};
use curatarr_core::error::DbError;
use curatarr_core::types::auth::{LoginAttempt, Session, User};
use curatarr_core::types::id::{SessionId, UserId};

pub(crate) async fn user_count(pool: &SqlitePool) -> Result<u64, DbError> {
    let row = sqlx::query("SELECT COUNT(*) as cnt FROM users")
        .fetch_one(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    let cnt: i64 = row.get("cnt");
    u64::try_from(cnt).map_err(|e| DbError::Internal(Box::new(e)))
}

pub(crate) async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<User, DbError> {
    let id = UserId::new();
    let now = now_iso();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at) VALUES (?, ?, ?, ?)")
        .bind(id.to_string())
        .bind(username)
        .bind(password_hash)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(crate::sqlite::map_unique_violation)?;
    get_user(pool, id).await?.ok_or(DbError::NotFound {
        entity: "user",
        id: id.to_string(),
    })
}

pub(crate) async fn get_user(pool: &SqlitePool, id: UserId) -> Result<Option<User>, DbError> {
    let row = sqlx::query("SELECT * FROM users WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(row.as_ref().map(user_from_row))
}

pub(crate) async fn get_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<User>, DbError> {
    let row = sqlx::query("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(row.as_ref().map(user_from_row))
}

pub(crate) async fn create_session(
    pool: &SqlitePool,
    user_id: UserId,
    csrf_token: &str,
    expires_at: DateTime<Utc>,
) -> Result<Session, DbError> {
    let id = SessionId::new();
    sqlx::query("INSERT INTO sessions (id, user_id, csrf_token, expires_at) VALUES (?, ?, ?, ?)")
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(csrf_token)
        .bind(expires_at.to_rfc3339())
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    get_session(pool, id).await?.ok_or(DbError::NotFound {
        entity: "session",
        id: id.to_string(),
    })
}

pub(crate) async fn get_session(
    pool: &SqlitePool,
    id: SessionId,
) -> Result<Option<Session>, DbError> {
    let row = sqlx::query("SELECT * FROM sessions WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(row.as_ref().map(session_from_row))
}

pub(crate) async fn revoke_session(pool: &SqlitePool, id: SessionId) -> Result<(), DbError> {
    sqlx::query("UPDATE sessions SET revoked_at = ? WHERE id = ?")
        .bind(now_iso())
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn get_login_attempt(
    pool: &SqlitePool,
    key: &str,
) -> Result<Option<LoginAttempt>, DbError> {
    let row = sqlx::query("SELECT * FROM login_attempts WHERE attempt_key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(row.as_ref().map(attempt_from_row))
}

pub(crate) async fn upsert_login_attempt(
    pool: &SqlitePool,
    key: &str,
    failures: i64,
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO login_attempts (attempt_key, failures, locked_until) VALUES (?, ?, ?)
         ON CONFLICT(attempt_key) DO UPDATE SET failures = excluded.failures, locked_until = excluded.locked_until",
    )
    .bind(key)
    .bind(failures)
    .bind(locked_until.map(|d| d.to_rfc3339()))
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn clear_login_attempt(pool: &SqlitePool, key: &str) -> Result<(), DbError> {
    sqlx::query("DELETE FROM login_attempts WHERE attempt_key = ?")
        .bind(key)
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, DbError> {
    let row = sqlx::query("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(row.map(|r| r.get("value")))
}

pub(crate) async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn list_settings(pool: &SqlitePool) -> Result<Vec<(String, String)>, DbError> {
    let rows = sqlx::query("SELECT key, value FROM app_settings ORDER BY key")
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(rows
        .iter()
        .map(|r| (r.get("key"), r.get("value")))
        .collect())
}

fn user_from_row(row: &sqlx::sqlite::SqliteRow) -> User {
    User {
        id: UserId::from_uuid(parse_uuid(row.get("id"))),
        username: row.get("username"),
        password_hash: row.get("password_hash"),
        created_at: parse_dt(row.get("created_at")),
    }
}

fn session_from_row(row: &sqlx::sqlite::SqliteRow) -> Session {
    Session {
        id: SessionId::from_uuid(parse_uuid(row.get("id"))),
        user_id: UserId::from_uuid(parse_uuid(row.get("user_id"))),
        csrf_token: row.get("csrf_token"),
        expires_at: parse_dt(row.get("expires_at")),
        revoked_at: parse_opt_dt(row.get("revoked_at")),
    }
}

fn attempt_from_row(row: &sqlx::sqlite::SqliteRow) -> LoginAttempt {
    LoginAttempt {
        attempt_key: row.get("attempt_key"),
        failures: row.get("failures"),
        locked_until: parse_opt_dt(row.get("locked_until")),
    }
}
