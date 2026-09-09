use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DownloadClientsConfig {
    #[serde(default)]
    pub qbittorrent: Option<QbittorrentConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QbittorrentConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub save_path: PathBuf,
}

impl QbittorrentConfig {
    pub fn enabled(&self) -> bool {
        !self.url.is_empty()
    }
}
