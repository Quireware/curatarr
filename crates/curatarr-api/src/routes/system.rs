use axum::Json;
use axum::extract::State;
use curatarr_core::traits::metadata_provider::ProviderHealth;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::ApiResult;
use crate::state::AppState;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    pub name: String,
    pub enabled: bool,
    pub priority: u32,
    pub content_types: Vec<curatarr_core::types::enums::ContentType>,
    pub health: ProviderHealth,
    pub success_rate: Option<f64>,
    pub last_latency_ms: u64,
}

pub async fn providers(State(state): State<AppState>) -> ApiResult<Json<Vec<ProviderStatus>>> {
    let mut out = Vec::new();
    for tracked in state.metadata.registry().all() {
        let health = tracked.health().await;
        out.push(ProviderStatus {
            name: tracked.name().to_string(),
            enabled: tracked.enabled,
            priority: tracked.priority,
            content_types: tracked.inner.supported_content_types().to_vec(),
            health,
            success_rate: tracked.success_rate(),
            last_latency_ms: tracked
                .last_latency_ms
                .load(std::sync::atomic::Ordering::Relaxed),
        });
    }
    Ok(Json(out))
}

pub async fn settings(State(state): State<AppState>) -> ApiResult<Json<BTreeMap<String, String>>> {
    Ok(Json(state.db.list_settings().await?.into_iter().collect()))
}

#[derive(Debug, Deserialize)]
pub struct SettingsBody {
    pub settings: BTreeMap<String, String>,
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsBody>,
) -> ApiResult<Json<BTreeMap<String, String>>> {
    for (key, value) in &body.settings {
        if looks_like_secret(key) {
            continue;
        }
        state.db.set_setting(key, value).await?;
    }
    settings(State(state)).await
}

pub async fn reload(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    if let Some(path) = &state.token_file {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| crate::error::ApiError::internal(format!("token file: {e}")))?;
        let token = raw.trim();
        if !token.is_empty() {
            *state.api_token.lock().unwrap_or_else(|e| e.into_inner()) = Arc::from(token);
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn looks_like_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("password") || lower.contains("token") || lower.contains("api_key")
}
