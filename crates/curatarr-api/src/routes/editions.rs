use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::edition::{Edition, EditionFilter, EditionUpdate, NewEdition};
use curatarr_core::types::enums::FileFormat;
use curatarr_core::types::file::{FileFilter, LibraryFile};
use curatarr_core::types::id::{EditionId, WorkId};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EditionListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub work_id: Option<WorkId>,
    pub format: Option<FileFormat>,
    pub language: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<EditionListQuery>,
) -> ApiResult<Json<Page<Edition>>> {
    let filter = EditionFilter {
        work_id: q.work_id,
        format: q.format,
        language: q.language,
    };
    let page = PageQuery {
        page: q.page,
        per_page: q.per_page,
    }
    .pagination();
    Ok(Json(state.db.list_editions(&filter, &page).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewEdition>,
) -> ApiResult<(StatusCode, Json<Edition>)> {
    if state.db.get_work(body.work_id).await?.is_none() {
        return Err(ApiError::not_found("work", body.work_id));
    }
    Ok((
        StatusCode::CREATED,
        Json(state.db.create_edition(&body).await?),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Edition>> {
    let id = EditionId::from_uuid(id);
    state
        .db
        .get_edition(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("edition", id))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<EditionUpdate>,
) -> ApiResult<Json<Edition>> {
    let id = EditionId::from_uuid(id);
    ensure_edition(&state, id).await?;
    Ok(Json(state.db.update_edition(id, &body).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let id = EditionId::from_uuid(id);
    ensure_edition(&state, id).await?;
    state.db.delete_edition(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn files(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<Page<LibraryFile>>> {
    let id = EditionId::from_uuid(id);
    ensure_edition(&state, id).await?;
    let filter = FileFilter {
        edition_id: Some(id),
        ..Default::default()
    };
    Ok(Json(state.db.list_files(&filter, &q.pagination()).await?))
}

async fn ensure_edition(state: &AppState, id: EditionId) -> ApiResult<()> {
    state
        .db
        .get_edition(id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found("edition", id))
}
