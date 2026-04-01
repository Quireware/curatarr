use axum::http::{Request, StatusCode};
use curatarr_api::router::build_router;
use curatarr_api::state::AppState;
use curatarr_db::create_repository;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn test_app() -> axum::Router {
    let db = create_repository("sqlite::memory:").await.unwrap();
    build_router(AppState { db })
}

#[tokio::test]
async fn health_returns_200() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn health_ready_returns_200_when_db_connected() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ready");
}
