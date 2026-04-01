mod sqlite;

pub use sqlite::SqliteRepository;

use curatarr_core::error::DbError;
use curatarr_core::traits::repository::Repository;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

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

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), DbError> {
    let migration_sql = include_str!("../../../migrations/sqlite/001_initial.sql");
    sqlx::raw_sql(migration_sql)
        .execute(pool)
        .await
        .map_err(|e| DbError::Migration(e.to_string()))?;
    Ok(())
}
