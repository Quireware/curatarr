use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::enums::SeriesType;
use curatarr_core::types::id::{SeriesEntryId, SeriesId, WorkId};
use curatarr_core::types::series::{
    NewSeries, NewSeriesEntry, Series, SeriesEntry, SeriesFilter, SeriesUpdate,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SeriesListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub title: Option<String>,
    pub series_type: Option<SeriesType>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<SeriesListQuery>,
) -> ApiResult<Json<Page<Series>>> {
    let filter = SeriesFilter {
        title_contains: q.title,
        series_type: q.series_type,
    };
    let page = PageQuery {
        page: q.page,
        per_page: q.per_page,
    }
    .pagination();
    Ok(Json(state.db.list_series(&filter, &page).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewSeries>,
) -> ApiResult<(StatusCode, Json<Series>)> {
    if body.title.trim().is_empty() {
        return Err(ApiError::bad_request("title must not be empty"));
    }
    Ok((
        StatusCode::CREATED,
        Json(state.db.create_series(&body).await?),
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Series>> {
    let id = SeriesId::from_uuid(id);
    state
        .db
        .get_series(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("series", id))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SeriesUpdate>,
) -> ApiResult<Json<Series>> {
    let id = SeriesId::from_uuid(id);
    if body.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
        return Err(ApiError::bad_request("title must not be empty"));
    }
    ensure_series(&state, id).await?;
    Ok(Json(state.db.update_series(id, &body).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let id = SeriesId::from_uuid(id);
    ensure_series(&state, id).await?;
    state.db.delete_series(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn entries(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<SeriesEntry>>> {
    let id = SeriesId::from_uuid(id);
    ensure_series(&state, id).await?;
    Ok(Json(state.db.list_series_entries(id).await?))
}

#[derive(Debug, Deserialize)]
pub struct AddEntryBody {
    pub work_id: WorkId,
    pub position: f64,
    pub arc: Option<String>,
}

pub async fn add_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddEntryBody>,
) -> ApiResult<(StatusCode, Json<SeriesEntry>)> {
    let id = SeriesId::from_uuid(id);
    ensure_series(&state, id).await?;
    if state.db.get_work(body.work_id).await?.is_none() {
        return Err(ApiError::not_found("work", body.work_id));
    }
    let entry = state
        .db
        .create_series_entry(&NewSeriesEntry {
            series_id: id,
            work_id: body.work_id,
            position: body.position,
            arc: body.arc,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn remove_entry(
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let id = SeriesId::from_uuid(id);
    let entry_id = SeriesEntryId::from_uuid(entry_id);
    ensure_series(&state, id).await?;
    let belongs = state
        .db
        .list_series_entries(id)
        .await?
        .iter()
        .any(|e| e.id == entry_id);
    if !belongs {
        return Err(ApiError::not_found("series entry", entry_id));
    }
    state.db.delete_series_entry(entry_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_series(state: &AppState, id: SeriesId) -> ApiResult<()> {
    state
        .db
        .get_series(id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found("series", id))
}
