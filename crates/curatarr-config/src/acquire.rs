use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcquireConfig {
    #[serde(default = "default_search_interval")]
    pub search_interval_minutes: u32,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u32,
    #[serde(default)]
    pub prowlarr_url: String,
    #[serde(default)]
    pub prowlarr_api_key: String,
    #[serde(default)]
    pub nzbget_url: String,
    #[serde(default)]
    pub nzbget_username: String,
    #[serde(default)]
    pub nzbget_password: String,
    #[serde(default)]
    pub nzbget_dest_dir: PathBuf,
    #[serde(default)]
    pub ntfy_url: String,
    #[serde(default)]
    pub ntfy_topic: String,
}

fn default_search_interval() -> u32 {
    15
}

fn default_poll_interval() -> u32 {
    10
}

impl Default for AcquireConfig {
    fn default() -> Self {
        Self {
            search_interval_minutes: default_search_interval(),
            poll_interval_seconds: default_poll_interval(),
            prowlarr_url: String::new(),
            prowlarr_api_key: String::new(),
            nzbget_url: String::new(),
            nzbget_username: String::new(),
            nzbget_password: String::new(),
            nzbget_dest_dir: PathBuf::new(),
            ntfy_url: String::new(),
            ntfy_topic: String::new(),
        }
    }
}

impl AcquireConfig {
    pub fn enabled(&self) -> bool {
        !self.prowlarr_url.is_empty() && !self.nzbget_url.is_empty()
    }
}
