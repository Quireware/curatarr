use axum::Json;
use axum::extract::{Query, State};
use curatarr_scanner::duplicates::{
    DEFAULT_NEAR_THRESHOLD, DuplicateGroup, NearDuplicate, find_duplicates, find_near_duplicates,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub async fn exact(State(state): State<AppState>) -> ApiResult<Json<Vec<DuplicateGroup>>> {
    Ok(Json(find_duplicates(state.db.as_ref()).await?))
}

#[derive(Debug, Deserialize)]
pub struct NearQuery {
    pub threshold: Option<f64>,
}

pub async fn near(
    State(state): State<AppState>,
    Query(q): Query<NearQuery>,
) -> ApiResult<Json<Vec<NearDuplicate>>> {
    let threshold = q.threshold.unwrap_or(DEFAULT_NEAR_THRESHOLD);
    if !(0.0..=1.0).contains(&threshold) {
        return Err(ApiError::bad_request("threshold must be between 0 and 1"));
    }
    Ok(Json(
        find_near_duplicates(state.db.as_ref(), threshold).await?,
    ))
}
