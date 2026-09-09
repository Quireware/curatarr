use axum::http::Method;
use axum::http::header::HeaderValue;

use crate::password::SESSION_COOKIE;

pub fn is_public_path(path: &str) -> bool {
    path == "/health"
        || path.starts_with("/health/")
        || path == "/api/v1/login"
        || path == "/api/v1/logout"
        || path == "/login"
        || path == "/favicon.ico"
}

pub fn is_ui_path(path: &str) -> bool {
    !path.starts_with("/api/") && !path.starts_with("/health")
}

pub fn is_mutating(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

pub fn session_id_from_cookies(header: Option<&HeaderValue>) -> Option<&str> {
    let text = header?.to_str().ok()?;
    for part in text.split(';') {
        let part = part.trim();
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        if name == SESSION_COOKIE && !value.is_empty() {
            return Some(value);
        }
    }
    None
}

pub fn session_cookie(session_id: &str, max_age_secs: i64) -> String {
    let mut header = String::from(SESSION_COOKIE);
    header.push('=');
    header.push_str(session_id);
    header.push_str("; HttpOnly; SameSite=Lax; Path=/; Max-Age=");
    header.push_str(&max_age_secs.to_string());
    header
}

pub fn clear_session_cookie() -> String {
    let mut header = String::from(SESSION_COOKIE);
    header.push_str("=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    header
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn login_and_health_are_public() {
        assert!(is_public_path("/api/v1/login"));
        assert!(is_public_path("/login"));
        assert!(is_public_path("/health/ready"));
        assert!(!is_public_path("/api/v1/works"));
        assert!(!is_public_path("/"));
    }

    #[test]
    fn parses_session_cookie() {
        let header = HeaderValue::from_static("a=1; curatarr_session=abc; b=2");
        assert_eq!(session_id_from_cookies(Some(&header)), Some("abc"));
        assert!(session_id_from_cookies(None).is_none());
    }
}
