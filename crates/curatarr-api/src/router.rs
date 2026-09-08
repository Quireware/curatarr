use axum::Router;
use axum::routing::{delete, get, post};
use tower_http::trace::TraceLayer;

use crate::routes::{
    authors, duplicates, editions, files, health, publishers, root_folders, series, works,
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

    Router::new()
        .route("/health", get(health::health))
        .route("/health/ready", get(health::ready))
        .nest("/api/v1", api)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
