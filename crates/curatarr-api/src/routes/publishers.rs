use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::id::PublisherId;
use curatarr_core::types::publisher::{NewPublisher, Publisher, PublisherFilter, PublisherUpdate};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PublisherListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub name: Option<String>,
    pub country: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<PublisherListQuery>,
) -> ApiResult<Json<Page<Publisher>>> {
    let filter = PublisherFilter {
        name_contains: q.name,
        country: q.country,
    };
    let page = PageQuery {
        page: q.page,
        per_page: q.per_page,
    }
    .pagination();
    Ok(Json(state.db.list_publishers(&filter, &page).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewPublisher>,
) -> ApiResult<(StatusCode, Json<Publisher>)> {
    if body.name.trim().is_empty() {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    Ok((
        StatusCode::CREATED,
        Json(state.db.create_publisher(&body).await?),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Publisher>> {
    let id = PublisherId::from_uuid(id);
    state
        .db
        .get_publisher(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("publisher", id))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PublisherUpdate>,
) -> ApiResult<Json<Publisher>> {
    let id = PublisherId::from_uuid(id);
    if body.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    ensure_publisher(&state, id).await?;
    Ok(Json(state.db.update_publisher(id, &body).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let id = PublisherId::from_uuid(id);
    ensure_publisher(&state, id).await?;
    state.db.delete_publisher(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_publisher(state: &AppState, id: PublisherId) -> ApiResult<()> {
    state
        .db
        .get_publisher(id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found("publisher", id))
}
