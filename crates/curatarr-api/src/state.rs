use chrono::{DateTime, Utc};
use curatarr_config::library::LibraryConfig;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::id::RootFolderId;
use curatarr_scanner::import::{ImportConfig, ImportEvent};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Repository>,
    pub library: Arc<LibraryConfig>,
    pub scans: ScanRegistry,
}

impl AppState {
    pub fn new(db: Arc<dyn Repository>, library: LibraryConfig) -> Self {
        Self {
            db,
            library: Arc::new(library),
            scans: ScanRegistry::default(),
        }
    }

    /// Import configuration for cataloguing `root` in place (root-folder scan).
    pub fn scan_config(&self, root: &Path) -> ImportConfig {
        ImportConfig {
            mode: self.library.import_mode,
            library_root: root.to_path_buf(),
            naming_template: self.library.naming_template.clone(), // clone: config is shared, ImportConfig is owned
            exclusions: self.library.exclusions.clone(), // clone: config is shared, ImportConfig is owned
            covers_dir: self.library.covers_dir(),
            organise: false,
        }
    }

    /// Import configuration for moving files from elsewhere into `root` with the naming template.
    pub fn import_config(&self, root: &Path) -> ImportConfig {
        ImportConfig {
            organise: true,
            ..self.scan_config(root)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanState {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanStatus {
    pub state: ScanState,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub total: usize,
    pub processed: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub error: Option<String>,
}

impl ScanStatus {
    fn started() -> Self {
        Self {
            state: ScanState::Running,
            started_at: Utc::now(),
            finished_at: None,
            total: 0,
            processed: 0,
            imported: 0,
            duplicates: 0,
            failed: 0,
            error: None,
        }
    }

    fn apply(&mut self, event: &ImportEvent) {
        match event {
            ImportEvent::Discovered { total } => self.total = *total,
            ImportEvent::Imported { .. } => {
                self.imported += 1;
                self.processed += 1;
            }
            ImportEvent::Duplicate { .. } => {
                self.duplicates += 1;
                self.processed += 1;
            }
            ImportEvent::Failed { .. } => {
                self.failed += 1;
                self.processed += 1;
            }
        }
    }
}

/// In-memory record of the latest scan per root folder.
#[derive(Clone, Default)]
pub struct ScanRegistry {
    inner: Arc<Mutex<HashMap<RootFolderId, ScanStatus>>>,
}

impl ScanRegistry {
    /// Mark a scan as started. Returns `false` if one is already running for this folder.
    pub fn try_start(&self, id: RootFolderId) -> bool {
        let mut map = self.lock();
        if map.get(&id).is_some_and(|s| s.state == ScanState::Running) {
            return false;
        }
        map.insert(id, ScanStatus::started());
        true
    }

    pub fn record(&self, id: RootFolderId, event: &ImportEvent) {
        if let Some(status) = self.lock().get_mut(&id) {
            status.apply(event);
        }
    }

    pub fn finish(&self, id: RootFolderId, error: Option<String>) {
        if let Some(status) = self.lock().get_mut(&id) {
            status.state = if error.is_some() {
                ScanState::Failed
            } else {
                ScanState::Completed
            };
            status.finished_at = Some(Utc::now());
            status.error = error;
        }
    }

    pub fn get(&self, id: RootFolderId) -> Option<ScanStatus> {
        self.lock().get(&id).cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<RootFolderId, ScanStatus>> {
        // A poisoned lock only means another scan task panicked mid-update; the map is still usable.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn registry_rejects_concurrent_scan_and_tracks_progress() {
        let reg = ScanRegistry::default();
        let id = RootFolderId::new();
        assert!(reg.try_start(id));
        assert!(!reg.try_start(id));

        reg.record(id, &ImportEvent::Discovered { total: 3 });
        reg.record(
            id,
            &ImportEvent::Imported {
                path: PathBuf::from("a"),
            },
        );
        reg.record(
            id,
            &ImportEvent::Duplicate {
                path: PathBuf::from("b"),
            },
        );
        reg.record(
            id,
            &ImportEvent::Failed {
                path: PathBuf::from("c"),
                error: "boom".into(),
            },
        );
        let status = reg.get(id).unwrap();
        assert_eq!(status.total, 3);
        assert_eq!(status.processed, 3);
        assert_eq!(
            (status.imported, status.duplicates, status.failed),
            (1, 1, 1)
        );

        reg.finish(id, None);
        let done = reg.get(id).unwrap();
        assert_eq!(done.state, ScanState::Completed);
        assert!(done.finished_at.is_some());
        assert!(reg.try_start(id));
    }

    #[test]
    fn finish_with_error_marks_failed() {
        let reg = ScanRegistry::default();
        let id = RootFolderId::new();
        reg.try_start(id);
        reg.finish(id, Some("disk gone".into()));
        let status = reg.get(id).unwrap();
        assert_eq!(status.state, ScanState::Failed);
        assert_eq!(status.error.as_deref(), Some("disk gone"));
    }
}
