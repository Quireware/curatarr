use axum::Router;
use axum::routing::get;
use tower_http::trace::TraceLayer;

use crate::routes::health;
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/health/ready", get(health::ready))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
