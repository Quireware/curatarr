use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use curatarr_core::types::id::WorkId;
use curatarr_core::types::queue::QueueItem;
use curatarr_core::types::release::Release;
use curatarr_core::types::work::{Work, WorkUpdate};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct MonitorBody {
    pub monitored: bool,
}

#[derive(Debug, Deserialize)]
pub struct GrabBody {
    pub guid: String,
}

pub async fn set_monitored(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<MonitorBody>,
) -> ApiResult<Json<Work>> {
    let work_id = WorkId::from_uuid(id);
    let work = state
        .db
        .update_work(
            work_id,
            &WorkUpdate {
                monitored: Some(body.monitored),
                ..Default::default()
            },
        )
        .await?;
    if body.monitored {
        if let Some(acquire) = &state.acquire {
            if let Err(e) = acquire.grab_work(work_id).await {
                tracing::warn!(error = %e, "auto-search after monitor failed");
            }
        }
    }
    Ok(Json(work))
}

pub async fn wanted(State(state): State<AppState>) -> ApiResult<Json<Vec<Work>>> {
    let mut works = Vec::new();
    for id in state.db.list_wanted_work_ids().await? {
        if let Some(work) = state.db.get_work(id).await? {
            works.push(work);
        }
    }
    Ok(Json(works))
}

pub async fn queue(State(state): State<AppState>) -> ApiResult<Json<Vec<QueueItem>>> {
    Ok(Json(state.db.list_queue().await?))
}

pub async fn search_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Release>>> {
    let Some(acquire) = &state.acquire else {
        return Err(ApiError::bad_request("acquisition is not configured"));
    };
    Ok(Json(acquire.search_work(WorkId::from_uuid(id)).await?))
}

pub async fn grab_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<GrabBody>,
) -> ApiResult<StatusCode> {
    let Some(acquire) = &state.acquire else {
        return Err(ApiError::bad_request("acquisition is not configured"));
    };
    acquire.grab_guid(WorkId::from_uuid(id), &body.guid).await?;
    Ok(StatusCode::ACCEPTED)
}
