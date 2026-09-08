use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use curatarr_core::types::id::RootFolderId;
use curatarr_core::types::root_folder::{NewRootFolder, RootFolder};
use curatarr_scanner::import::{ImportReport, import_directory, import_directory_with_progress};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, ScanStatus};

#[derive(Debug, Serialize)]
pub struct RootFolderResponse {
    #[serde(flatten)]
    pub folder: RootFolder,
    /// Whether the directory currently exists on disk.
    pub accessible: bool,
    pub free_space_bytes: Option<u64>,
    pub total_space_bytes: Option<u64>,
}

fn describe(folder: RootFolder) -> RootFolderResponse {
    let path = PathBuf::from(&folder.path);
    let accessible = path.is_dir();
    let (free_space_bytes, total_space_bytes) = if accessible {
        (
            fs2::available_space(&path).ok(),
            fs2::total_space(&path).ok(),
        )
    } else {
        (None, None)
    };
    RootFolderResponse {
        folder,
        accessible,
        free_space_bytes,
        total_space_bytes,
    }
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<RootFolderResponse>>> {
    let folders = state.db.list_root_folders().await?;
    Ok(Json(folders.into_iter().map(describe).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewRootFolder>,
) -> ApiResult<(StatusCode, Json<RootFolderResponse>)> {
    let path = PathBuf::from(body.path.trim());
    if !path.is_absolute() {
        return Err(ApiError::bad_request("root folder path must be absolute"));
    }
    if !path.is_dir() {
        return Err(ApiError::bad_request(format!(
            "root folder does not exist or is not a directory: {}",
            path.display()
        )));
    }
    let folder = state
        .db
        .create_root_folder(&NewRootFolder {
            path: path.to_string_lossy().into_owned(),
            name: body.name,
            content_types: body.content_types,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(describe(folder))))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RootFolderResponse>> {
    let folder = load(&state, RootFolderId::from_uuid(id)).await?;
    Ok(Json(describe(folder)))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let id = RootFolderId::from_uuid(id);
    load(&state, id).await?;
    state.db.delete_root_folder(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct ScanAccepted {
    pub root_folder_id: RootFolderId,
    pub status_url: String,
}

/// Start an in-place scan of the folder in the background. Poll `scan/status` for progress.
pub async fn scan(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<ScanAccepted>)> {
    let id = RootFolderId::from_uuid(id);
    let folder = load(&state, id).await?;
    let root = PathBuf::from(&folder.path);
    if !root.is_dir() {
        return Err(ApiError::bad_request(format!(
            "root folder is not accessible: {}",
            root.display()
        )));
    }
    if !state.scans.try_start(id) {
        return Err(ApiError::conflict(
            "a scan is already running for this root folder",
        ));
    }

    let config = state.scan_config(&root);
    let task_state = state.clone(); // clone: AppState is cheap (Arcs) and moves into the task
    tokio::spawn(async move {
        let scans = task_state.scans.clone(); // clone: registry handle shared with the progress closure
        let result =
            import_directory_with_progress(&root, &config, task_state.db.as_ref(), move |event| {
                scans.record(id, event)
            })
            .await;
        match result {
            Ok(report) => {
                tracing::info!(
                    root = %root.display(),
                    imported = report.imported.len(),
                    duplicates = report.duplicates.len(),
                    failed = report.failed.len(),
                    "scan finished"
                );
                task_state.scans.finish(id, None);
            }
            Err(e) => {
                tracing::error!(root = %root.display(), error = %e, "scan failed");
                task_state.scans.finish(id, Some(e.to_string()));
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ScanAccepted {
            root_folder_id: id,
            status_url: format!("/api/v1/root-folders/{id}/scan/status"),
        }),
    ))
}

pub async fn scan_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ScanStatus>> {
    let id = RootFolderId::from_uuid(id);
    load(&state, id).await?;
    state
        .scans
        .get(id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("scan for root folder", id))
}

#[derive(Debug, Deserialize)]
pub struct ImportBody {
    /// Directory outside the library whose files should be organised into this root folder.
    pub source: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub failures: Vec<ImportFailureResponse>,
}

#[derive(Debug, Serialize)]
pub struct ImportFailureResponse {
    pub path: PathBuf,
    pub error: String,
}

impl From<ImportReport> for ImportSummary {
    fn from(report: ImportReport) -> Self {
        Self {
            imported: report.imported.len(),
            duplicates: report.duplicates.len(),
            failed: report.failed.len(),
            failures: report
                .failed
                .into_iter()
                .map(|f| ImportFailureResponse {
                    path: f.path,
                    error: f.error,
                })
                .collect(),
        }
    }
}

/// Synchronously import an external directory into this root folder using the naming template.
pub async fn import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ImportBody>,
) -> ApiResult<Json<ImportSummary>> {
    let id = RootFolderId::from_uuid(id);
    let folder = load(&state, id).await?;
    if !body.source.is_absolute() || !body.source.is_dir() {
        return Err(ApiError::bad_request(
            "source must be an absolute path to an existing directory",
        ));
    }
    let root = PathBuf::from(&folder.path);
    let config = state.import_config(&root);
    let report = import_directory(&body.source, &config, state.db.as_ref()).await?;
    Ok(Json(report.into()))
}

async fn load(state: &AppState, id: RootFolderId) -> ApiResult<RootFolder> {
    state
        .db
        .get_root_folder(id)
        .await?
        .ok_or_else(|| ApiError::not_found("root folder", id))
}
