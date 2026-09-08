use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::author::Author;
use curatarr_core::types::edition::{Edition, EditionFilter};
use curatarr_core::types::enums::{AgeRating, AuthorRole, ContentType, ReadStatus};
use curatarr_core::types::id::{AuthorId, WorkId};
use curatarr_core::types::series::SeriesEntry;
use curatarr_core::types::work::{NewWork, Work, WorkFilter, WorkUpdate};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct WorkListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub content_type: Option<ContentType>,
    pub read_status: Option<ReadStatus>,
    pub age_rating: Option<AgeRating>,
    pub title: Option<String>,
    pub language: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<WorkListQuery>,
) -> ApiResult<Json<Page<Work>>> {
    let filter = WorkFilter {
        content_type: q.content_type,
        read_status: q.read_status,
        age_rating: q.age_rating,
        title_contains: q.title,
        language: q.language,
    };
    let page = PageQuery {
        page: q.page,
        per_page: q.per_page,
    }
    .pagination();
    Ok(Json(state.db.list_works(&filter, &page).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewWork>,
) -> ApiResult<(StatusCode, Json<Work>)> {
    if body.title.trim().is_empty() {
        return Err(ApiError::bad_request("title must not be empty"));
    }
    let work = state.db.create_work(&body).await?;
    Ok((StatusCode::CREATED, Json(work)))
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Work>> {
    let id = WorkId::from_uuid(id);
    state
        .db
        .get_work(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("work", id))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<WorkUpdate>,
) -> ApiResult<Json<Work>> {
    let id = WorkId::from_uuid(id);
    if body.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
        return Err(ApiError::bad_request("title must not be empty"));
    }
    ensure_work(&state, id).await?;
    Ok(Json(state.db.update_work(id, &body).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let id = WorkId::from_uuid(id);
    ensure_work(&state, id).await?;
    state.db.delete_work(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn authors(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Author>>> {
    let id = WorkId::from_uuid(id);
    ensure_work(&state, id).await?;
    Ok(Json(state.db.list_work_authors(id).await?))
}

#[derive(Debug, Deserialize)]
pub struct LinkAuthorBody {
    pub author_id: AuthorId,
    #[serde(default = "default_role")]
    pub role: AuthorRole,
}

fn default_role() -> AuthorRole {
    AuthorRole::Author
}

pub async fn link_author(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LinkAuthorBody>,
) -> ApiResult<StatusCode> {
    let id = WorkId::from_uuid(id);
    ensure_work(&state, id).await?;
    if state.db.get_author(body.author_id).await?.is_none() {
        return Err(ApiError::not_found("author", body.author_id));
    }
    state
        .db
        .link_work_author(id, body.author_id, body.role)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unlink_author(
    State(state): State<AppState>,
    Path((id, author_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let id = WorkId::from_uuid(id);
    ensure_work(&state, id).await?;
    state
        .db
        .unlink_work_author(id, AuthorId::from_uuid(author_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn editions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<Page<Edition>>> {
    let id = WorkId::from_uuid(id);
    ensure_work(&state, id).await?;
    let filter = EditionFilter {
        work_id: Some(id),
        ..Default::default()
    };
    Ok(Json(
        state.db.list_editions(&filter, &q.pagination()).await?,
    ))
}

pub async fn series_entries(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<SeriesEntry>>> {
    let id = WorkId::from_uuid(id);
    ensure_work(&state, id).await?;
    Ok(Json(state.db.list_work_series_entries(id).await?))
}

async fn ensure_work(state: &AppState, id: WorkId) -> ApiResult<()> {
    state
        .db
        .get_work(id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found("work", id))
}
