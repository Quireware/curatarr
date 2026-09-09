use axum::extract::{Path, State};
use axum::response::Html;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use curatarr_core::types::id::WorkId;
use curatarr_web::{QueueCard, WorkCard};

pub async fn login_page() -> Html<String> {
    Html(curatarr_web::login_page())
}

pub async fn library_page(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let page = state
        .db
        .list_works(
            &curatarr_core::types::work::WorkFilter::default(),
            &curatarr_core::types::Pagination {
                page: 1,
                per_page: 100,
            },
        )
        .await?;
    let cards: Vec<WorkCard> = page
        .items
        .iter()
        .map(|w| WorkCard {
            id: w.id.to_string(),
            title: w.title.clone(),
            content_type: format!("{:?}", w.content_type).to_ascii_lowercase(),
        })
        .collect();
    Ok(Html(curatarr_web::library_page(&cards)))
}

pub async fn wanted_page(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let mut cards = Vec::new();
    for id in state.db.list_wanted_work_ids().await? {
        if let Some(w) = state.db.get_work(id).await? {
            cards.push(WorkCard {
                id: w.id.to_string(),
                title: w.title,
                content_type: format!("{:?}", w.content_type).to_ascii_lowercase(),
            });
        }
    }
    Ok(Html(curatarr_web::wanted_page(&cards)))
}

pub async fn queue_page(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let items = state.db.list_queue().await?;
    let cards: Vec<QueueCard> = items
        .iter()
        .map(|q| QueueCard {
            title: q.title.clone(),
            state: format!("{:?}", q.state).to_ascii_lowercase(),
        })
        .collect();
    Ok(Html(curatarr_web::queue_page(&cards)))
}

pub async fn work_page(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Html<String>> {
    let work_id = WorkId::from_uuid(id);
    let work = state
        .db
        .get_work(work_id)
        .await?
        .ok_or_else(|| ApiError::not_found("work", work_id))?;
    let card = WorkCard {
        id: work.id.to_string(),
        title: work.title,
        content_type: format!("{:?}", work.content_type).to_ascii_lowercase(),
    };
    Ok(Html(curatarr_web::work_page(&card, work.monitored)))
}

pub async fn settings_page(State(state): State<AppState>) -> ApiResult<Html<String>> {
    let rows = state.db.list_settings().await?;
    Ok(Html(curatarr_web::settings_page(&rows)))
}
