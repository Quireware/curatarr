use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbBackend {
    Sqlite,
    Postgres,
}

impl Default for DbBackend {
    fn default() -> Self {
        Self::Sqlite
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub backend: DbBackend,
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: DbBackend::default(),
            url: default_url(),
            max_connections: default_max_connections(),
        }
    }
}

fn default_url() -> String {
    "sqlite://curatarr.db?mode=rwc".into()
}

fn default_max_connections() -> u32 {
    5
}
