use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub username: String,
    pub csrf_token: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> ApiResult<impl IntoResponse> {
    let now = Utc::now();
    if let Some(attempt) = state.db.get_login_attempt(&body.username).await? {
        if curatarr_auth::is_locked(attempt.locked_until, now) {
            return Err(ApiError::locked("too many failed logins; try again later"));
        }
    }
    let user = match state.db.get_user_by_username(&body.username).await? {
        Some(user) => user,
        None => {
            record_failure(&state, &body.username, now).await?;
            return Err(ApiError::unauthorized());
        }
    };
    let ok = curatarr_auth::verify_password(&body.password, &user.password_hash)
        .map_err(|_| ApiError::internal("password verify failed"))?;
    if !ok {
        record_failure(&state, &body.username, now).await?;
        return Err(ApiError::unauthorized());
    }
    state.db.clear_login_attempt(&body.username).await?;
    let csrf = Uuid::now_v7().to_string();
    let session = state
        .db
        .create_session(user.id, &csrf, curatarr_auth::session_expiry(now))
        .await?;
    let cookie = curatarr_auth::session_cookie(
        &session.id.to_string(),
        curatarr_auth::SESSION_DAYS.saturating_mul(86_400),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ApiError::internal("cookie header"))?,
    );
    Ok((
        StatusCode::OK,
        headers,
        Json(LoginResponse {
            username: user.username,
            csrf_token: csrf,
        }),
    ))
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(id) = curatarr_auth::session_id_from_cookies(headers.get(header::COOKIE)) {
        if let Ok(id) = id.parse() {
            if let Err(e) = state.db.revoke_session(id).await {
                tracing::warn!(error = %e, "revoke session failed");
            }
        }
    }
    let mut out = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&curatarr_auth::clear_session_cookie()) {
        out.insert(header::SET_COOKIE, value);
    }
    (StatusCode::NO_CONTENT, out)
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<LoginResponse>> {
    let Some(raw) = curatarr_auth::session_id_from_cookies(headers.get(header::COOKIE)) else {
        return Err(ApiError::unauthorized());
    };
    let id = raw.parse().map_err(|_| ApiError::unauthorized())?;
    let Some(session) = state.db.get_session(id).await? else {
        return Err(ApiError::unauthorized());
    };
    if session.revoked_at.is_some() || session.expires_at <= Utc::now() {
        return Err(ApiError::unauthorized());
    }
    let Some(user) = state.db.get_user(session.user_id).await? else {
        return Err(ApiError::unauthorized());
    };
    Ok(Json(LoginResponse {
        username: user.username,
        csrf_token: session.csrf_token,
    }))
}

async fn record_failure(
    state: &AppState,
    username: &str,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    let current = state
        .db
        .get_login_attempt(username)
        .await?
        .map(|a| u32::try_from(a.failures).unwrap_or(u32::MAX))
        .unwrap_or(0);
    let (failures, locked) = curatarr_auth::register_failure(current, now);
    state
        .db
        .upsert_login_attempt(username, i64::from(failures), locked)
        .await?;
    Ok(())
}
