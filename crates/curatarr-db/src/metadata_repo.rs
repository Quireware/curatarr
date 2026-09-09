use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::sqlite::{now_iso, parse_dt};
use curatarr_core::error::DbError;
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{
    AuditEvent, EntityKind, FieldLock, FieldSource, NewAuditEvent,
};
use curatarr_core::types::{Page, Pagination};

pub(crate) async fn list_external_ids(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
) -> Result<Vec<ExternalId>, DbError> {
    let rows = sqlx::query(
        "SELECT provider, external_id FROM external_ids
         WHERE entity_type = ? AND entity_id = ?",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    let mut ids = Vec::new();
    for row in rows {
        let provider: String = row.get("provider");
        let value: String = row.get("external_id");
        if let Ok(id) = ExternalId::from_provider(&provider, &value) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub(crate) async fn upsert_external_id(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
    id: &ExternalId,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO external_ids (entity_type, entity_id, provider, external_id)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(entity_type, entity_id, provider) DO UPDATE SET external_id = excluded.external_id",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .bind(id.storage_key())
    .bind(id.value())
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn list_field_locks(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
) -> Result<Vec<FieldLock>, DbError> {
    let rows = sqlx::query(
        "SELECT field, locked_at FROM metadata_field_locks
         WHERE entity_type = ? AND entity_id = ? ORDER BY field",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    Ok(rows
        .iter()
        .map(|row| FieldLock {
            entity_kind: kind,
            entity_id: entity_id.to_string(),
            field: row.get("field"),
            locked_at: parse_dt(row.get("locked_at")),
        })
        .collect())
}

pub(crate) async fn set_field_lock(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
    field: &str,
) -> Result<FieldLock, DbError> {
    let now = now_iso();
    sqlx::query(
        "INSERT INTO metadata_field_locks (entity_type, entity_id, field, locked_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(entity_type, entity_id, field) DO UPDATE SET locked_at = excluded.locked_at",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .bind(field)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    Ok(FieldLock {
        entity_kind: kind,
        entity_id: entity_id.to_string(),
        field: field.to_string(),
        locked_at: parse_dt(&now),
    })
}

pub(crate) async fn clear_field_lock(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
    field: &str,
) -> Result<(), DbError> {
    sqlx::query(
        "DELETE FROM metadata_field_locks
         WHERE entity_type = ? AND entity_id = ? AND field = ?",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .bind(field)
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn clear_all_field_locks(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM metadata_field_locks WHERE entity_type = ? AND entity_id = ?")
        .bind(kind.as_str())
        .bind(entity_id)
        .execute(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn list_field_sources(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
) -> Result<Vec<FieldSource>, DbError> {
    let rows = sqlx::query(
        "SELECT field, source FROM metadata_field_sources
         WHERE entity_type = ? AND entity_id = ? ORDER BY field",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    Ok(rows
        .iter()
        .map(|row| FieldSource {
            entity_kind: kind,
            entity_id: entity_id.to_string(),
            field: row.get("field"),
            source: row.get("source"),
        })
        .collect())
}

pub(crate) async fn upsert_field_source(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
    field: &str,
    source: &str,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO metadata_field_sources (entity_type, entity_id, field, source)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(entity_type, entity_id, field) DO UPDATE SET source = excluded.source",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .bind(field)
    .bind(source)
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;
    Ok(())
}

pub(crate) async fn append_audit(
    pool: &SqlitePool,
    event: &NewAuditEvent,
) -> Result<AuditEvent, DbError> {
    let id = Uuid::now_v7().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO metadata_audit_log
         (id, entity_type, entity_id, field, old_value, new_value, source, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(event.entity_kind.as_str())
    .bind(&event.entity_id)
    .bind(&event.field)
    .bind(&event.old_value)
    .bind(&event.new_value)
    .bind(&event.source)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    Ok(AuditEvent {
        id,
        entity_kind: event.entity_kind,
        entity_id: event.entity_id.clone(), // clone: persist row owns a copy of the entity id
        field: event.field.clone(),         // clone: persist row owns a copy of the field name
        old_value: event.old_value.clone(), // clone: persist row owns a copy of the optional value
        new_value: event.new_value.clone(), // clone: persist row owns a copy of the optional value
        source: event.source.clone(),       // clone: persist row owns a copy of the source name
        created_at: parse_dt(&now),
    })
}

pub(crate) async fn list_audit(
    pool: &SqlitePool,
    kind: EntityKind,
    entity_id: &str,
    page: &Pagination,
) -> Result<Page<AuditEvent>, DbError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM metadata_audit_log WHERE entity_type = ? AND entity_id = ?",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .fetch_one(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    let offset = i64::from(page.page.saturating_sub(1)) * i64::from(page.per_page);
    let rows = sqlx::query(
        "SELECT id, field, old_value, new_value, source, created_at
         FROM metadata_audit_log
         WHERE entity_type = ? AND entity_id = ?
         ORDER BY created_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(kind.as_str())
    .bind(entity_id)
    .bind(i64::from(page.per_page))
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| DbError::Internal(Box::new(e)))?;

    let items = rows
        .iter()
        .map(|row| AuditEvent {
            id: row.get("id"),
            entity_kind: kind,
            entity_id: entity_id.to_string(),
            field: row.get("field"),
            old_value: row.get("old_value"),
            new_value: row.get("new_value"),
            source: row.get("source"),
            created_at: parse_dt(row.get("created_at")),
        })
        .collect();

    Ok(Page {
        items,
        total: u64::try_from(total).unwrap_or(0),
        page: page.page,
        per_page: page.per_page,
    })
}

pub(crate) async fn find_author_id(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<String>, DbError> {
    sqlx::query_scalar("SELECT id FROM authors WHERE lower(name) = lower(?) LIMIT 1")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))
}

pub(crate) async fn find_series_id(
    pool: &SqlitePool,
    title: &str,
) -> Result<Option<String>, DbError> {
    sqlx::query_scalar("SELECT id FROM series WHERE lower(title) = lower(?) LIMIT 1")
        .bind(title)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))
}

pub(crate) async fn find_tag_id(pool: &SqlitePool, name: &str) -> Result<Option<String>, DbError> {
    sqlx::query_scalar("SELECT id FROM tags WHERE lower(name) = lower(?) LIMIT 1")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))
}
