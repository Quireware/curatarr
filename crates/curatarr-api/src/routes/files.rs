use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::enums::FileFormat;
use curatarr_core::types::file::{FileFilter, LibraryFile};
use curatarr_core::types::id::{EditionId, FileId};
use curatarr_core::types::recycle::RecycleEntry;
use curatarr_scanner::recycle;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct FileListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub edition_id: Option<EditionId>,
    pub format: Option<FileFormat>,
    #[serde(default)]
    pub include_deleted: bool,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<FileListQuery>,
) -> ApiResult<Json<Page<LibraryFile>>> {
    let filter = FileFilter {
        edition_id: q.edition_id,
        format: q.format,
        include_deleted: q.include_deleted,
    };
    let page = PageQuery {
        page: q.page,
        per_page: q.per_page,
    }
    .pagination();
    Ok(Json(state.db.list_files(&filter, &page).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<LibraryFile>> {
    let id = FileId::from_uuid(id);
    state
        .db
        .get_file(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("file", id))
}

#[derive(Debug, Deserialize)]
pub struct RemoveQuery {
    #[serde(default)]
    pub confirm: bool,
}

/// Permanent delete. Requires `?confirm=true`. A recycled file is purged from disk as well;
/// an active file only loses its database record (the file on disk is left alone).
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<RemoveQuery>,
) -> ApiResult<StatusCode> {
    let id = FileId::from_uuid(id);
    if !q.confirm {
        return Err(ApiError::bad_request(
            "permanent delete requires ?confirm=true; use /recycle for a reversible delete",
        ));
    }
    if state.db.get_file(id).await?.is_none() {
        return Err(ApiError::not_found("file", id));
    }
    if state.db.get_recycle_entry_for_file(id).await?.is_some() {
        recycle::purge(id, state.db.as_ref()).await?;
    } else {
        state.db.delete_file(id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn recycle(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RecycleEntry>> {
    let id = FileId::from_uuid(id);
    let entry = recycle::soft_delete(id, &state.library.recycle_dir(), state.db.as_ref()).await?;
    Ok(Json(entry))
}

pub async fn restore(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<LibraryFile>> {
    let id = FileId::from_uuid(id);
    Ok(Json(recycle::restore(id, state.db.as_ref()).await?))
}

pub async fn recycle_bin(State(state): State<AppState>) -> ApiResult<Json<Vec<RecycleEntry>>> {
    Ok(Json(state.db.list_recycle_entries().await?))
}

#[derive(Debug, Deserialize)]
pub struct CleanupQuery {
    /// Overrides the configured retention period.
    pub retention_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    pub purged: u64,
    pub retention_days: u32,
}

pub async fn cleanup_recycle_bin(
    State(state): State<AppState>,
    Query(q): Query<CleanupQuery>,
) -> ApiResult<Json<CleanupResponse>> {
    let retention_days = q
        .retention_days
        .unwrap_or(state.library.recycle_retention_days);
    let purged = recycle::cleanup_recycle_bin(retention_days, state.db.as_ref()).await?;
    Ok(Json(CleanupResponse {
        purged,
        retention_days,
    }))
}
