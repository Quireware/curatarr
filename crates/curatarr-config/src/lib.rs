pub mod database;
pub mod library;
pub mod logging;
pub mod server;

use curatarr_core::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::path::Path;

use database::DatabaseConfig;
use library::LibraryConfig;
use logging::LogConfig;
use server::ServerConfig;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub library: LibraryConfig,
    #[serde(default)]
    pub log: LogConfig,
}

impl AppConfig {
    /// Load configuration from optional TOML file with environment variable overlay.
    ///
    /// Env vars use prefix `CURATARR` with `__` as separator.
    /// Example: `CURATARR__SERVER__PORT=9090`
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(config_path) = path {
            if !config_path.exists() {
                return Err(ConfigError::LoadFailed(format!(
                    "config file not found: {}",
                    config_path.display()
                )));
            }
            builder = builder.add_source(config::File::from(config_path).required(true));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("CURATARR")
                .separator("__")
                .try_parsing(true),
        );

        let config = builder
            .build()
            .map_err(|e| ConfigError::LoadFailed(e.to_string()))?;

        config
            .try_deserialize()
            .map_err(|e| ConfigError::LoadFailed(e.to_string()))
    }

    /// Load configuration with defaults only (no file, no env).
    pub fn defaults() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DbBackend;
    use crate::logging::LogFormat;
    use proptest::prelude::*;
    use rstest::rstest;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_toml(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::with_suffix(".toml").unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn defaults_produce_valid_config() {
        let config = AppConfig::defaults();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8787);
        assert_eq!(config.database.backend, DbBackend::Sqlite);
        assert_eq!(config.log.format, LogFormat::Pretty);
    }

    #[rstest]
    #[case(
        r#"
[server]
host = "127.0.0.1"
port = 9090

[database]
backend = "postgres"
url = "postgres://localhost/curatarr"
max_connections = 10

[log]
level = "debug"
format = "json"
"#,
        "127.0.0.1",
        9090,
        DbBackend::Postgres,
        10,
        "debug",
        LogFormat::Json
    )]
    #[case(
        r#"
[server]
port = 3000
"#,
        "0.0.0.0",
        3000,
        DbBackend::Sqlite,
        5,
        "info",
        LogFormat::Pretty
    )]
    fn load_from_toml(
        #[case] toml_content: &str,
        #[case] expected_host: &str,
        #[case] expected_port: u16,
        #[case] expected_backend: DbBackend,
        #[case] expected_max_conn: u32,
        #[case] expected_log_level: &str,
        #[case] expected_log_format: LogFormat,
    ) {
        let file = write_toml(toml_content);
        let config = AppConfig::load(Some(file.path())).unwrap();
        assert_eq!(config.server.host, expected_host);
        assert_eq!(config.server.port, expected_port);
        assert_eq!(config.database.backend, expected_backend);
        assert_eq!(config.database.max_connections, expected_max_conn);
        assert_eq!(config.log.level, expected_log_level);
        assert_eq!(config.log.format, expected_log_format);
    }

    #[test]
    fn load_missing_file_returns_error() {
        let result = AppConfig::load(Some(Path::new("/nonexistent/config.toml")));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "Error was: {err}");
    }

    #[test]
    fn load_invalid_toml_returns_error() {
        let file = write_toml("this is not valid = = = toml{{{}}}");
        let result = AppConfig::load(Some(file.path()));
        assert!(result.is_err());
    }

    #[test]
    fn load_no_file_uses_defaults() {
        let config = AppConfig::load(None).unwrap();
        assert_eq!(config.server.port, 8787);
        assert_eq!(config.database.backend, DbBackend::Sqlite);
    }

    #[test]
    fn env_var_override() {
        unsafe { std::env::set_var("CURATARR__SERVER__PORT", "9999") };
        let config = AppConfig::load(None).unwrap();
        unsafe { std::env::remove_var("CURATARR__SERVER__PORT") };
        assert_eq!(config.server.port, 9999);
    }

    proptest! {
        #[test]
        fn server_config_serde_roundtrip(port in 1u16..=65534, host in "[a-z0-9.]{1,20}") {
            let config = ServerConfig { host, port };
            let toml_str = toml::to_string(&config).unwrap();
            let back: ServerConfig = toml::from_str(&toml_str).unwrap();
            prop_assert_eq!(config, back);
        }

        #[test]
        fn database_config_serde_roundtrip(max_conn in 1u32..100) {
            let config = DatabaseConfig {
                backend: DbBackend::Sqlite,
                url: "sqlite://test.db".into(),
                max_connections: max_conn,
            };
            let toml_str = toml::to_string(&config).unwrap();
            let back: DatabaseConfig = toml::from_str(&toml_str).unwrap();
            prop_assert_eq!(config, back);
        }
    }

    #[test]
    fn library_config_with_root_folders() {
        let toml_content = r#"
[library]
naming_template = "{Author}/{Title}.{Extension}"
import_mode = "move"
exclusions = ["*.tmp", ".DS_Store"]

[[library.root_folders]]
path = "/books"
name = "Main Library"

[[library.root_folders]]
path = "/manga"
content_types = ["manga", "comic"]
"#;
        let file = write_toml(toml_content);
        let config = AppConfig::load(Some(file.path())).unwrap();
        assert_eq!(config.library.root_folders.len(), 2);
        assert_eq!(
            config.library.naming_template,
            "{Author}/{Title}.{Extension}"
        );
        assert_eq!(
            config.library.import_mode,
            curatarr_core::types::enums::ImportMode::Move
        );
        assert_eq!(config.library.exclusions.len(), 2);
    }
}
