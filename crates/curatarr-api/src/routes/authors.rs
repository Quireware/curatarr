use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::author::{Author, AuthorFilter, AuthorUpdate, NewAuthor};
use curatarr_core::types::id::AuthorId;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AuthorListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub name: Option<String>,
    pub nationality: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<AuthorListQuery>,
) -> ApiResult<Json<Page<Author>>> {
    let filter = AuthorFilter {
        name_contains: q.name,
        nationality: q.nationality,
    };
    let page = PageQuery {
        page: q.page,
        per_page: q.per_page,
    }
    .pagination();
    Ok(Json(state.db.list_authors(&filter, &page).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewAuthor>,
) -> ApiResult<(StatusCode, Json<Author>)> {
    if body.name.trim().is_empty() {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    Ok((
        StatusCode::CREATED,
        Json(state.db.create_author(&body).await?),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Author>> {
    let id = AuthorId::from_uuid(id);
    state
        .db
        .get_author(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("author", id))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AuthorUpdate>,
) -> ApiResult<Json<Author>> {
    let id = AuthorId::from_uuid(id);
    if body.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    ensure_author(&state, id).await?;
    Ok(Json(state.db.update_author(id, &body).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let id = AuthorId::from_uuid(id);
    ensure_author(&state, id).await?;
    state.db.delete_author(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_author(state: &AppState, id: AuthorId) -> ApiResult<()> {
    state
        .db
        .get_author(id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found("author", id))
}
