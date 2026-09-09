use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use curatarr_core::error::{DbError, ProviderError, ScannerError};
use curatarr_metadata::EnrichError;
use serde_json::json;

/// Every error leaves the API as `{ "error": { "code": ..., "message": ... } }`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn not_found(entity: &str, id: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND",
            message: format!("{entity} {id} not found"),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "BAD_REQUEST",
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "CONFLICT",
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL",
            message: message.into(),
        }
    }

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "RATE_LIMITED",
            message: message.into(),
        }
    }
}

impl From<ProviderError> for ApiError {
    fn from(e: ProviderError) -> Self {
        match e {
            ProviderError::NotFound { provider, id } => Self::not_found(&provider, id),
            ProviderError::MissingApiKey { .. }
            | ProviderError::Disabled { .. }
            | ProviderError::Unsupported { .. } => Self::bad_request(e.to_string()),
            ProviderError::RateLimited { .. } => Self::too_many_requests(e.to_string()),
            ProviderError::Request { .. } | ProviderError::Parse { .. } => {
                tracing::error!(error = %e, "provider error");
                Self::internal(e.to_string())
            }
        }
    }
}

impl From<EnrichError> for ApiError {
    fn from(e: EnrichError) -> Self {
        match e {
            EnrichError::Db(db) => db.into(),
            EnrichError::Provider(p) => p.into(),
            EnrichError::WorkNotFound(id) => Self::not_found("work", id),
            EnrichError::AuthorNotFound(id) => Self::not_found("author", id),
            EnrichError::SeriesNotFound(id) => Self::not_found("series", id),
            EnrichError::UnknownField(field) => Self::bad_request(format!("unknown field {field}")),
        }
    }
}

impl From<DbError> for ApiError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::NotFound { entity, id } => Self::not_found(entity, id),
            DbError::Conflict(msg) => Self::conflict(msg),
            DbError::Migration(msg) => Self::internal(msg),
            DbError::Internal(err) => {
                tracing::error!(error = %err, "database error");
                Self::internal("database error")
            }
        }
    }
}

impl From<ScannerError> for ApiError {
    fn from(e: ScannerError) -> Self {
        match e {
            ScannerError::Database(db) => db.into(),
            ScannerError::AlreadyRecycled(id) => {
                Self::conflict(format!("file {id} is already in the recycle bin"))
            }
            ScannerError::NotRecycled(id) => {
                Self::conflict(format!("file {id} is not in the recycle bin"))
            }
            ScannerError::InvalidTemplate { .. } | ScannerError::InvalidExclusion { .. } => {
                Self::bad_request(e.to_string())
            }
            ScannerError::Io { .. }
            | ScannerError::UnsupportedFormat(_)
            | ScannerError::ExtractionFailed { .. }
            | ScannerError::HashMismatch { .. } => {
                tracing::error!(error = %e, "scanner error");
                Self::internal(e.to_string())
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        });
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_not_found_maps_to_404() {
        let err: ApiError = DbError::NotFound {
            entity: "work",
            id: "abc".into(),
        }
        .into();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.code, "NOT_FOUND");
        assert!(err.message.contains("abc"));
    }

    #[test]
    fn db_conflict_maps_to_409() {
        let err: ApiError = DbError::Conflict("dup".into()).into();
        assert_eq!(err.status, StatusCode::CONFLICT);
    }

    #[test]
    fn scanner_db_error_unwraps() {
        let err: ApiError = ScannerError::Database(DbError::NotFound {
            entity: "file",
            id: "x".into(),
        })
        .into();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn already_recycled_is_conflict() {
        let err: ApiError = ScannerError::AlreadyRecycled("x".into()).into();
        assert_eq!(err.status, StatusCode::CONFLICT);
    }
}
