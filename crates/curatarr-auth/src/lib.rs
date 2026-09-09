pub mod cookie;
pub mod password;

pub use cookie::{
    clear_session_cookie, is_mutating, is_public_path, is_ui_path, session_cookie,
    session_id_from_cookies,
};
pub use password::{
    CSRF_HEADER, LOCK_MINUTES, MAX_LOGIN_FAILURES, PasswordError, SESSION_COOKIE, SESSION_DAYS,
    hash_password, is_locked, register_failure, session_expiry, verify_password,
};

use axum::http::header::HeaderValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Missing,
    Invalid,
    Locked,
}

/// Constant-time comparison. An empty expected token never matches.
pub fn token_matches(expected: &str, provided: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let a = expected.as_bytes();
    let b = provided.as_bytes();
    let max = a.len().max(b.len());
    let mut diff = a.len() ^ b.len();
    for i in 0..max {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

pub fn check_bearer(expected: &str, header: Option<&HeaderValue>) -> Result<(), AuthError> {
    let Some(value) = header else {
        return Err(AuthError::Missing);
    };
    let Ok(text) = value.to_str() else {
        return Err(AuthError::Invalid);
    };
    let Some(provided) = text.strip_prefix("Bearer ") else {
        return Err(AuthError::Invalid);
    };
    if token_matches(expected, provided) {
        Ok(())
    } else {
        Err(AuthError::Invalid)
    }
}

pub fn check_csrf(expected: &str, header: Option<&HeaderValue>) -> Result<(), AuthError> {
    let Some(value) = header else {
        return Err(AuthError::Missing);
    };
    let Ok(provided) = value.to_str() else {
        return Err(AuthError::Invalid);
    };
    if token_matches(expected, provided) {
        Ok(())
    } else {
        Err(AuthError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn missing_header_is_rejected() {
        assert_eq!(check_bearer("secret", None), Err(AuthError::Missing));
    }

    #[test]
    fn matching_bearer_is_accepted() {
        let header = HeaderValue::from_static("Bearer secret");
        assert_eq!(check_bearer("secret", Some(&header)), Ok(()));
    }

    #[test]
    fn wrong_token_is_rejected() {
        let header = HeaderValue::from_static("Bearer other");
        assert_eq!(
            check_bearer("secret", Some(&header)),
            Err(AuthError::Invalid)
        );
    }

    #[test]
    fn empty_expected_never_matches() {
        let header = HeaderValue::from_static("Bearer ");
        assert_eq!(check_bearer("", Some(&header)), Err(AuthError::Invalid));
        assert!(!token_matches("", ""));
    }

    #[test]
    fn csrf_matches() {
        let header = HeaderValue::from_static("tok");
        assert_eq!(check_csrf("tok", Some(&header)), Ok(()));
        assert_eq!(check_csrf("tok", None), Err(AuthError::Missing));
    }
}
