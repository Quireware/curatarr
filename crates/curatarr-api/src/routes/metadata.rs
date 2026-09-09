use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use curatarr_core::types::Page;
use curatarr_core::types::id::{AuthorId, SeriesId, WorkId};
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{
    AuditEvent, EntityKind, FieldLock, FieldSource, MetadataMatch, MetadataQuery, RefreshReport,
    fields,
};
use curatarr_core::types::work::WorkFilter;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::routes::common::PageQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RefreshQuery {
    pub preview: Option<bool>,
}

pub async fn refresh_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<RefreshQuery>,
) -> ApiResult<Json<RefreshReport>> {
    let preview = q.preview.unwrap_or(false);
    Ok(Json(
        state
            .metadata
            .refresh_work(WorkId::from_uuid(id), preview)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct BulkRefreshBody {
    pub content_type: Option<curatarr_core::types::enums::ContentType>,
    pub preview: Option<bool>,
}

pub async fn refresh_bulk(
    State(state): State<AppState>,
    Json(body): Json<BulkRefreshBody>,
) -> ApiResult<Json<Vec<RefreshReport>>> {
    let filter = WorkFilter {
        content_type: body.content_type,
        ..WorkFilter::default()
    };
    Ok(Json(
        state
            .metadata
            .bulk_refresh(&filter, body.preview.unwrap_or(false))
            .await?,
    ))
}

pub async fn search(
    State(state): State<AppState>,
    Json(query): Json<MetadataQuery>,
) -> ApiResult<Json<Vec<MetadataMatch>>> {
    Ok(Json(state.metadata.search(&query).await))
}

#[derive(Debug, Deserialize)]
pub struct ApplyBody {
    pub provider: String,
    pub external_id: ExternalId,
    pub preview: Option<bool>,
}

pub async fn apply_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ApplyBody>,
) -> ApiResult<Json<RefreshReport>> {
    Ok(Json(
        state
            .metadata
            .apply_match(
                WorkId::from_uuid(id),
                &body.provider,
                &body.external_id,
                body.preview.unwrap_or(false),
            )
            .await?,
    ))
}

pub async fn list_locks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<FieldLock>>> {
    let work_id = WorkId::from_uuid(id);
    ensure_work(&state, work_id).await?;
    Ok(Json(
        state
            .db
            .list_field_locks(EntityKind::Work, &work_id.to_string())
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct LocksBody {
    pub fields: Vec<String>,
}

pub async fn replace_locks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LocksBody>,
) -> ApiResult<Json<Vec<FieldLock>>> {
    state
        .metadata
        .set_locks(WorkId::from_uuid(id), &body.fields)
        .await?;
    list_locks(State(state), Path(id)).await
}

#[derive(Debug, Deserialize)]
pub struct LockFieldBody {
    pub field: String,
}

pub async fn add_lock(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LockFieldBody>,
) -> ApiResult<(StatusCode, Json<FieldLock>)> {
    if !fields::is_known(&body.field) {
        return Err(ApiError::bad_request(format!(
            "unknown field {}",
            body.field
        )));
    }
    let work_id = WorkId::from_uuid(id);
    ensure_work(&state, work_id).await?;
    let lock = state
        .db
        .set_field_lock(EntityKind::Work, &work_id.to_string(), &body.field)
        .await?;
    Ok((StatusCode::CREATED, Json(lock)))
}

pub async fn remove_lock(
    State(state): State<AppState>,
    Path((id, field)): Path<(Uuid, String)>,
) -> ApiResult<StatusCode> {
    let work_id = WorkId::from_uuid(id);
    ensure_work(&state, work_id).await?;
    state
        .db
        .clear_field_lock(EntityKind::Work, &work_id.to_string(), &field)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_audit(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(page): Query<PageQuery>,
) -> ApiResult<Json<Page<AuditEvent>>> {
    let work_id = WorkId::from_uuid(id);
    ensure_work(&state, work_id).await?;
    Ok(Json(
        state
            .db
            .list_audit(EntityKind::Work, &work_id.to_string(), &page.pagination())
            .await?,
    ))
}

pub async fn list_sources(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<FieldSource>>> {
    let work_id = WorkId::from_uuid(id);
    ensure_work(&state, work_id).await?;
    Ok(Json(
        state
            .db
            .list_field_sources(EntityKind::Work, &work_id.to_string())
            .await?,
    ))
}

pub async fn import_author_works(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<RefreshQuery>,
) -> ApiResult<Json<Vec<RefreshReport>>> {
    Ok(Json(
        state
            .metadata
            .import_author_works(AuthorId::from_uuid(id), q.preview.unwrap_or(false))
            .await?,
    ))
}

pub async fn import_series_works(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<RefreshQuery>,
) -> ApiResult<Json<Vec<RefreshReport>>> {
    Ok(Json(
        state
            .metadata
            .import_series_works(SeriesId::from_uuid(id), q.preview.unwrap_or(false))
            .await?,
    ))
}

async fn ensure_work(state: &AppState, id: WorkId) -> ApiResult<()> {
    state
        .db
        .get_work(id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found("work", id))
}
