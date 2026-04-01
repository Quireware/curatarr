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
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            root_folders: Vec::new(),
            naming_template: default_naming_template(),
            import_mode: ImportMode::Copy,
            exclusions: Vec::new(),
        }
    }
}

fn default_naming_template() -> String {
    "{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}".into()
}
