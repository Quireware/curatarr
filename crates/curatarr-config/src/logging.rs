use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Pretty,
}

impl Default for LogFormat {
    fn default() -> Self {
        Self::Pretty
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default)]
    pub format: LogFormat,
    pub file: Option<PathBuf>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: LogFormat::default(),
            file: None,
        }
    }
}

fn default_level() -> String {
    "info".into()
}
