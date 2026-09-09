use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    Empty,
    Hash,
    Verify,
}

pub const MAX_LOGIN_FAILURES: u32 = 10;
pub const LOCK_MINUTES: i64 = 15;
pub const SESSION_DAYS: i64 = 7;
pub const SESSION_COOKIE: &str = "curatarr_session";
pub const CSRF_HEADER: &str = "x-csrf-token";

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| PasswordError::Hash)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    if password.is_empty() || hash.is_empty() {
        return Ok(false);
    }
    let parsed = PasswordHash::new(hash).map_err(|_| PasswordError::Verify)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn is_locked(locked_until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    locked_until.is_some_and(|until| until > now)
}

pub fn register_failure(failures: u32, now: DateTime<Utc>) -> (u32, Option<DateTime<Utc>>) {
    let next = failures.saturating_add(1);
    let lock = if next >= MAX_LOGIN_FAILURES {
        Some(now + Duration::minutes(LOCK_MINUTES))
    } else {
        None
    };
    (next, lock)
}

pub fn session_expiry(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::days(SESSION_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn empty_password_is_rejected() {
        assert_eq!(hash_password(""), Err(PasswordError::Empty));
        assert!(!verify_password("", "x").unwrap());
    }

    #[rstest]
    #[case(0, false)]
    #[case(8, false)]
    #[case(9, true)]
    fn tenth_failure_locks(#[case] start: u32, #[case] locked: bool) {
        let now = Utc::now();
        let (next, until) = register_failure(start, now);
        assert_eq!(next, start + 1);
        assert_eq!(until.is_some(), locked);
        if locked {
            assert!(is_locked(until, now));
            assert!(!is_locked(until, now + Duration::minutes(LOCK_MINUTES + 1)));
        }
    }
}
