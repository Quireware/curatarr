use curatarr_core::types::enums::{ContentType, ImportMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RootFolderConfig {
    pub path: PathBuf,
    pub name: Option<String>,
    pub content_types: Option<Vec<ContentType>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryConfig {
    #[serde(default)]
    pub root_folders: Vec<RootFolderConfig>,
    #[serde(default = "default_naming_template")]
    pub naming_template: String,
    #[serde(default)]
    pub import_mode: ImportMode,
    #[serde(default)]
    pub exclusions: Vec<String>,
    /// Where curatarr keeps its own files: covers under `covers/`, recycle bin under `recycle/`.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Days a soft-deleted file stays in the recycle bin before it is purged.
    #[serde(default = "default_recycle_retention_days")]
    pub recycle_retention_days: u32,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            root_folders: Vec::new(),
            naming_template: default_naming_template(),
            import_mode: ImportMode::Copy,
            exclusions: Vec::new(),
            data_dir: default_data_dir(),
            recycle_retention_days: default_recycle_retention_days(),
        }
    }
}

impl LibraryConfig {
    pub fn covers_dir(&self) -> PathBuf {
        self.data_dir.join("covers")
    }

    pub fn recycle_dir(&self) -> PathBuf {
        self.data_dir.join("recycle")
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

fn default_recycle_retention_days() -> u32 {
    30
}

fn default_naming_template() -> String {
    "{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}".into()
}
