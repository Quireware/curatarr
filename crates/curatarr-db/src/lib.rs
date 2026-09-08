mod sqlite;

pub use sqlite::SqliteRepository;

use curatarr_core::error::DbError;
use curatarr_core::traits::repository::Repository;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use std::sync::Arc;

/// Embedded SQLite migrations, applied once each in order and recorded in `_migrations`.
const MIGRATIONS: &[(&str, &str)] = &[(
    "001_initial",
    include_str!("../../../migrations/sqlite/001_initial.sql"),
)];

pub async fn create_repository(url: &str) -> Result<Arc<dyn Repository>, DbError> {
    let options = SqliteConnectOptions::from_str(url)
        .map_err(|e| DbError::Internal(Box::new(e)))?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

    run_migrations(&pool).await?;

    Ok(Arc::new(SqliteRepository::new(pool)))
}

/// Apply every embedded migration that has not been recorded yet. Safe to call on every start.
pub async fn run_migrations(pool: &SqlitePool) -> Result<Vec<String>, DbError> {
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| DbError::Migration(e.to_string()))?;

    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations")
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Migration(e.to_string()))?
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    let mut newly_applied = Vec::new();
    for (name, sql) in MIGRATIONS {
        if applied.iter().any(|a| a == name) {
            continue;
        }
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| DbError::Migration(e.to_string()))?;
        sqlx::raw_sql(sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| DbError::Migration(format!("{name}: {e}")))?;
        sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(|e| DbError::Migration(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| DbError::Migration(e.to_string()))?;
        tracing::info!(migration = name, "applied migration");
        newly_applied.push((*name).to_string());
    }
    Ok(newly_applied)
}
