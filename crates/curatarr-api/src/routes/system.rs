use axum::Json;
use axum::extract::State;
use curatarr_core::traits::metadata_provider::ProviderHealth;
use serde::Serialize;

use crate::error::ApiResult;
use crate::state::AppState;

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
