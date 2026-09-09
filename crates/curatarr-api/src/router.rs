use axum::Router;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post};
use chrono::Utc;
use tower_http::trace::TraceLayer;

use crate::routes::{
    acquire, auth, authors, duplicates, editions, files, health, metadata, publishers,
    root_folders, series, system, ui, works,
};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        // works
        .route("/works", get(works::list).post(works::create))
        .route(
            "/works/{id}",
            get(works::get_one).put(works::update).delete(works::remove),
        )
        .route(
            "/works/{id}/authors",
            get(works::authors).post(works::link_author),
        )
        .route(
            "/works/{id}/authors/{author_id}",
            delete(works::unlink_author),
        )
        .route("/works/{id}/editions", get(works::editions))
        .route("/works/{id}/series", get(works::series_entries))
        .route("/works/{id}/refresh-metadata", post(metadata::refresh_one))
        .route("/works/{id}/apply-metadata", post(metadata::apply_one))
        .route(
            "/works/{id}/field-locks",
            get(metadata::list_locks)
                .put(metadata::replace_locks)
                .post(metadata::add_lock),
        )
        .route(
            "/works/{id}/field-locks/{field}",
            delete(metadata::remove_lock),
        )
        .route("/works/{id}/metadata-audit", get(metadata::list_audit))
        .route("/works/{id}/metadata-sources", get(metadata::list_sources))
        .route("/works/refresh-metadata", post(metadata::refresh_bulk))
        .route("/metadata/search", post(metadata::search))
        .route("/system/providers", get(system::providers))
        .route(
            "/system/settings",
            get(system::settings).put(system::put_settings),
        )
        .route("/system/reload", post(system::reload))
        .route("/login", post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/me", get(auth::me))
        .route("/works/{id}/monitor", post(acquire::set_monitored))
        .route("/works/{id}/search", post(acquire::search_one))
        .route("/works/{id}/grab", post(acquire::grab_one))
        .route("/wanted", get(acquire::wanted))
        .route("/queue", get(acquire::queue))
        // editions
        .route("/editions", get(editions::list).post(editions::create))
        .route(
            "/editions/{id}",
            get(editions::get_one)
                .put(editions::update)
                .delete(editions::remove),
        )
        .route("/editions/{id}/files", get(editions::files))
        // authors
        .route("/authors", get(authors::list).post(authors::create))
        .route(
            "/authors/{id}",
            get(authors::get_one)
                .put(authors::update)
                .delete(authors::remove),
        )
        .route(
            "/authors/{id}/import-works",
            post(metadata::import_author_works),
        )
        // series
        .route("/series", get(series::list).post(series::create))
        .route(
            "/series/{id}",
            get(series::get_one)
                .put(series::update)
                .delete(series::remove),
        )
        .route(
            "/series/{id}/entries",
            get(series::entries).post(series::add_entry),
        )
        .route(
            "/series/{id}/entries/{entry_id}",
            delete(series::remove_entry),
        )
        .route(
            "/series/{id}/import-works",
            post(metadata::import_series_works),
        )
        // publishers
        .route(
            "/publishers",
            get(publishers::list).post(publishers::create),
        )
        .route(
            "/publishers/{id}",
            get(publishers::get_one)
                .put(publishers::update)
                .delete(publishers::remove),
        )
        // files + recycle bin
        .route("/files", get(files::list))
        .route("/files/{id}", get(files::get_one).delete(files::remove))
        .route("/files/{id}/recycle", post(files::recycle))
        .route("/files/{id}/restore", post(files::restore))
        .route("/recycle-bin", get(files::recycle_bin))
        .route("/recycle-bin/cleanup", post(files::cleanup_recycle_bin))
        // root folders + scanning
        .route(
            "/root-folders",
            get(root_folders::list).post(root_folders::create),
        )
        .route(
            "/root-folders/{id}",
            get(root_folders::get_one).delete(root_folders::remove),
        )
        .route("/root-folders/{id}/scan", post(root_folders::scan))
        .route(
            "/root-folders/{id}/scan/status",
            get(root_folders::scan_status),
        )
        .route("/root-folders/{id}/import", post(root_folders::import))
        // duplicates
        .route("/duplicates", get(duplicates::exact))
        .route("/duplicates/near", get(duplicates::near));

    let auth_state = state.clone(); // clone: middleware holds AppState for token checks
    Router::new()
        .route("/health", get(health::health))
        .route("/health/ready", get(health::ready))
        .route("/login", get(ui::login_page))
        .route("/", get(ui::library_page))
        .route("/wanted", get(ui::wanted_page))
        .route("/queue", get(ui::queue_page))
        .route("/works/{id}", get(ui::work_page))
        .route("/settings", get(ui::settings_page))
        .nest("/api/v1", api)
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            require_auth,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    if curatarr_auth::is_public_path(&path) {
        return next.run(request).await;
    }
    let token = state.current_token();
    if curatarr_auth::check_bearer(token.as_ref(), request.headers().get(header::AUTHORIZATION))
        .is_ok()
    {
        return next.run(request).await;
    }
    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let csrf = request
        .headers()
        .get(curatarr_auth::CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    match authenticate_session(&state, cookie.as_deref(), csrf.as_deref(), &method).await {
        Ok(()) => next.run(request).await,
        Err(resp) => {
            if curatarr_auth::is_ui_path(&path) && method == axum::http::Method::GET {
                Redirect::to("/login").into_response()
            } else {
                resp
            }
        }
    }
}

async fn authenticate_session(
    state: &AppState,
    cookie: Option<&str>,
    csrf: Option<&str>,
    method: &axum::http::Method,
) -> Result<(), Response> {
    let unauthorized = || crate::error::ApiError::unauthorized().into_response();
    let header = cookie.and_then(|c| axum::http::HeaderValue::from_str(c).ok());
    let Some(raw) = curatarr_auth::session_id_from_cookies(header.as_ref()) else {
        return Err(unauthorized());
    };
    let Ok(id) = raw.parse() else {
        return Err(unauthorized());
    };
    let session = match state.db.get_session(id).await {
        Ok(Some(session)) => session,
        Ok(None) => return Err(unauthorized()),
        Err(_) => return Err(crate::error::ApiError::internal("session lookup").into_response()),
    };
    if session.revoked_at.is_some() || session.expires_at <= Utc::now() {
        return Err(unauthorized());
    }
    if curatarr_auth::is_mutating(method) {
        let csrf_header = csrf.and_then(|c| axum::http::HeaderValue::from_str(c).ok());
        if curatarr_auth::check_csrf(&session.csrf_token, csrf_header.as_ref()).is_err() {
            return Err(crate::error::ApiError::forbidden("csrf token required").into_response());
        }
    }
    Ok(())
}
