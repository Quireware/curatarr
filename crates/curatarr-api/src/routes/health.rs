use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

use crate::state::AppState;

pub async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_tags().await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "unavailable", "reason": "database not reachable"})),
        ),
    }
}
