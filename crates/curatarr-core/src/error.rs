use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid ISBN: {value} — {reason}")]
    InvalidIsbn { value: String, reason: String },

    #[error("invalid identifier: {value} — {reason}")]
    InvalidIdentifier { value: String, reason: String },

    #[error("unsupported content type: {0}")]
    UnsupportedContentType(String),

    #[error("validation failed: {field} — {reason}")]
    Validation { field: String, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration load failed: {0}")]
    LoadFailed(String),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: String, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("entity not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("unique constraint violated: {0}")]
    Conflict(String),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("database error: {0}")]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, thiserror::Error)]
pub enum ScannerError {
    #[error("I/O error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("unsupported file format: {0}")]
    UnsupportedFormat(String),

    #[error("metadata extraction failed for {}: {reason}", path.display())]
    ExtractionFailed { path: PathBuf, reason: String },

    #[error("hash mismatch for {}: expected {expected}, got {actual}", path.display())]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;
    use std::io;

    fn arb_non_empty_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_ ]{1,50}"
    }

    fn arb_path() -> impl Strategy<Value = PathBuf> {
        "[a-zA-Z0-9_/]{1,50}".prop_map(PathBuf::from)
    }

    proptest! {
        #[test]
        fn core_error_display_non_empty(
            value in arb_non_empty_string(),
            reason in arb_non_empty_string(),
            field in arb_non_empty_string(),
        ) {
            let errors: Vec<CoreError> = vec![
                CoreError::InvalidIsbn { value: value.clone(), reason: reason.clone() },
                CoreError::InvalidIdentifier { value: value.clone(), reason: reason.clone() },
                CoreError::UnsupportedContentType(value.clone()),
                CoreError::Validation { field, reason },
            ];
            for e in &errors {
                let display = e.to_string();
                prop_assert!(!display.is_empty(), "Display was empty for {:?}", e);
            }
        }

        #[test]
        fn config_error_display_non_empty(
            msg in arb_non_empty_string(),
            field in arb_non_empty_string(),
            reason in arb_non_empty_string(),
        ) {
            let errors: Vec<ConfigError> = vec![
                ConfigError::LoadFailed(msg),
                ConfigError::MissingField(field.clone()),
                ConfigError::InvalidValue { field, reason },
            ];
            for e in &errors {
                let display = e.to_string();
                prop_assert!(!display.is_empty(), "Display was empty for {:?}", e);
            }
        }

        #[test]
        fn db_error_display_non_empty(
            id in arb_non_empty_string(),
            msg in arb_non_empty_string(),
        ) {
            let errors: Vec<DbError> = vec![
                DbError::NotFound { entity: "work", id },
                DbError::Conflict(msg.clone()),
                DbError::Migration(msg),
            ];
            for e in &errors {
                let display = e.to_string();
                prop_assert!(!display.is_empty(), "Display was empty for {:?}", e);
            }
        }

        #[test]
        fn scanner_error_display_non_empty(
            path in arb_path(),
            reason in arb_non_empty_string(),
            expected in arb_non_empty_string(),
            actual in arb_non_empty_string(),
        ) {
            let errors: Vec<ScannerError> = vec![
                ScannerError::UnsupportedFormat(reason.clone()),
                ScannerError::ExtractionFailed { path: path.clone(), reason },
                ScannerError::HashMismatch { path, expected, actual },
            ];
            for e in &errors {
                let display = e.to_string();
                prop_assert!(!display.is_empty(), "Display was empty for {:?}", e);
            }
        }
    }

    #[rstest]
    #[case(
        CoreError::InvalidIsbn { value: "123".into(), reason: "wrong length".into() },
        "invalid ISBN: 123"
    )]
    #[case(
        CoreError::Validation { field: "title".into(), reason: "cannot be empty".into() },
        "validation failed: title"
    )]
    #[case(
        DbError::NotFound { entity: "work", id: "abc-123".into() },
        "entity not found: work abc-123"
    )]
    #[case(
        DbError::Conflict("duplicate title".into()),
        "unique constraint violated: duplicate title"
    )]
    #[case(
        ScannerError::UnsupportedFormat("txt".into()),
        "unsupported file format: txt"
    )]
    fn error_messages_contain_expected_substring<E: std::error::Error>(
        #[case] error: E,
        #[case] expected_substring: &str,
    ) {
        let display = error.to_string();
        assert!(
            display.contains(expected_substring),
            "Expected '{}' to contain '{}'",
            display,
            expected_substring
        );
    }

    #[test]
    fn db_error_internal_wraps_source() {
        let source = io::Error::new(io::ErrorKind::ConnectionRefused, "db down");
        let err = DbError::Internal(Box::new(source));
        assert!(err.to_string().contains("db down"));
    }

    #[test]
    fn scanner_io_error_includes_path_and_source() {
        let source = io::Error::new(io::ErrorKind::NotFound, "file missing");
        let err = ScannerError::Io {
            path: PathBuf::from("/books/test.epub"),
            source,
        };
        let display = err.to_string();
        assert!(display.contains("/books/test.epub"));
        assert!(display.contains("file missing"));
    }
}
