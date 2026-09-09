use crate::score::ranked;
use crate::upgrade::should_upgrade;
use curatarr_core::error::DbError;
use curatarr_core::traits::download_client::{DownloadClient, DownloadRequest};
use curatarr_core::traits::indexer::{Indexer, IndexerProtocol};
use curatarr_core::traits::notifier::{Notification, Notifier};
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::edition::EditionFilter;
use curatarr_core::types::enums::{DownloadState, FileFormat, ImportMode};
use curatarr_core::types::file::{FileFilter, LibraryFile};
use curatarr_core::types::id::WorkId;
use curatarr_core::types::profile::{DEFAULT_QUALITY_PROFILE_ID, QualityProfile};
use curatarr_core::types::queue::{NewQueueItem, QueueItem, QueueItemUpdate};
use curatarr_core::types::release::{IndexerQuery, Release};
use curatarr_core::types::work::WorkFilter;
use curatarr_scanner::format::detect_format;
use curatarr_scanner::import::{ImportConfig, TargetedImport, import_file_for_work};
use curatarr_scanner::recycle::soft_delete;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AcquireSettings {
    pub library_root: PathBuf,
    pub covers_dir: PathBuf,
    pub recycle_dir: PathBuf,
    pub naming_template: String,
    pub import_mode: ImportMode,
    pub download_roots: Vec<PathBuf>,
}

pub struct AcquireService {
    db: Arc<dyn Repository>,
    indexer: Arc<dyn Indexer>,
    downloader: Arc<dyn DownloadClient>,
    torrent: Option<Arc<dyn DownloadClient>>,
    notifier: Option<Arc<dyn Notifier>>,
    settings: AcquireSettings,
}

impl AcquireService {
    pub fn new(
        db: Arc<dyn Repository>,
        indexer: Arc<dyn Indexer>,
        downloader: Arc<dyn DownloadClient>,
        settings: AcquireSettings,
    ) -> Self {
        Self {
            db,
            indexer,
            downloader,
            torrent: None,
            notifier: None,
            settings,
        }
    }

    pub fn with_notifier(mut self, notifier: Arc<dyn Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    pub fn with_torrent(mut self, torrent: Arc<dyn DownloadClient>) -> Self {
        self.torrent = Some(torrent);
        self
    }

    pub async fn search_wanted(&self) -> Result<usize, DbError> {
        let mut grabbed = 0;
        let mut page = 1u32;
        const PER_PAGE: u32 = 100;
        loop {
            let result = self
                .db
                .list_works(
                    &WorkFilter::default(),
                    &Pagination {
                        page,
                        per_page: PER_PAGE,
                    },
                )
                .await?;
            for work in result.items {
                if !work.monitored {
                    continue;
                }
                if self.db.active_queue_for_work(work.id).await?.is_some() {
                    continue;
                }
                if self.grab_work(work.id).await? {
                    grabbed += 1;
                }
            }
            let seen = u64::from(page.saturating_mul(PER_PAGE));
            if seen >= result.total {
                break;
            }
            page = page.saturating_add(1);
        }
        Ok(grabbed)
    }

    pub async fn search_work(&self, work_id: WorkId) -> Result<Vec<Release>, DbError> {
        let Some(work) = self.db.get_work(work_id).await? else {
            return Err(DbError::NotFound {
                entity: "work",
                id: work_id.to_string(),
            });
        };
        self.indexer
            .search(&IndexerQuery {
                query: work.title.clone(), // clone: indexer query owns the title
                content_type: Some(work.content_type),
            })
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))
    }

    pub async fn grab_work(&self, work_id: WorkId) -> Result<bool, DbError> {
        if self.db.active_queue_for_work(work_id).await?.is_some() {
            return Ok(false);
        }
        let hits = self.searchable_releases(work_id).await?;
        let profile = self.load_profile().await?;
        let existing = self.existing_formats(work_id).await?;
        for release in ranked(&hits, &profile) {
            if !existing.is_empty() && !should_upgrade(&existing, release, &profile) {
                continue;
            }
            if self.submit_release(work_id, release).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn searchable_releases(&self, work_id: WorkId) -> Result<Vec<Release>, DbError> {
        let hits = self.search_work(work_id).await?;
        Ok(hits
            .into_iter()
            .filter(|r| self.downloader_for(r.protocol).is_some())
            .collect())
    }

    pub async fn grab_guid(&self, work_id: WorkId, guid: &str) -> Result<bool, DbError> {
        if self.db.active_queue_for_work(work_id).await?.is_some() {
            return Ok(false);
        }
        let hits = self.search_work(work_id).await?;
        let Some(release) = hits.iter().find(|r| r.guid == guid) else {
            return Ok(false);
        };
        self.submit_release(work_id, release).await
    }

    fn downloader_for(&self, protocol: IndexerProtocol) -> Option<&dyn DownloadClient> {
        match protocol {
            IndexerProtocol::Newznab => Some(self.downloader.as_ref()),
            IndexerProtocol::Torznab => self.torrent.as_deref(),
            IndexerProtocol::Rss => None,
        }
    }

    fn client_for_item(&self, item: &QueueItem) -> Option<&dyn DownloadClient> {
        let name = item.client.as_deref().unwrap_or("");
        if self.downloader.name() == name {
            return Some(self.downloader.as_ref());
        }
        if let Some(torrent) = &self.torrent {
            if torrent.name() == name {
                return Some(torrent.as_ref());
            }
        }
        self.downloader_for(item.protocol.unwrap_or(IndexerProtocol::Newznab))
    }

    pub async fn reconcile(&self) -> Result<(), DbError> {
        let mut downloads = match self.downloader.list_downloads().await {
            Ok(list) => list,
            Err(e) => {
                tracing::warn!(error = %e, "reconcile list_downloads failed");
                Vec::new()
            }
        };
        if let Some(torrent) = &self.torrent {
            match torrent.list_downloads().await {
                Ok(list) => downloads.extend(list),
                Err(e) => tracing::warn!(error = %e, "reconcile torrent list_downloads failed"),
            }
        }
        for item in self.db.list_queue().await? {
            if item.state != DownloadState::Submitting {
                continue;
            }
            let Some(matched) = downloads.iter().find(|d| matches_queue_item(d, &item)) else {
                continue;
            };
            let update = QueueItemUpdate {
                state: Some(DownloadState::Grabbed),
                client_ref: Some(matched.client_ref.clone()), // clone: queue row stores client id
                ..Default::default()
            };
            if self
                .db
                .cas_queue_state(item.id, item.revision, &update)
                .await?
                .is_none()
            {
                tracing::warn!(queue = %item.id, "reconcile cas lost");
            }
        }
        Ok(())
    }

    pub async fn poll_queue(&self) -> Result<(), DbError> {
        for item in self.db.list_queue().await? {
            self.poll_item(item).await?;
        }
        Ok(())
    }

    async fn submit_release(&self, work_id: WorkId, release: &Release) -> Result<bool, DbError> {
        let item = match self
            .db
            .create_queue_item(&NewQueueItem {
                work_id,
                state: DownloadState::Submitting,
                protocol: Some(release.protocol),
                indexer: Some(release.indexer.clone()), // clone: queue row stores indexer name
                title: release.title.clone(),           // clone: queue row stores release title
                guid: Some(release.guid.clone()),       // clone: queue row stores guid
                download_url: Some(release.download_url.as_str().to_string()),
                client: Some(
                    self.downloader_for(release.protocol)
                        .map(|c| c.name().to_string())
                        .unwrap_or_default(),
                ),
                idempotency_key: String::new(),
            })
            .await
        {
            Ok(item) => item,
            Err(DbError::Conflict(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        let Some(client) = self.downloader_for(release.protocol) else {
            self.mark_failed(&item, "no download client for protocol".into())
                .await?;
            return Ok(false);
        };
        let request = DownloadRequest {
            name: item.idempotency_key.clone(), // clone: NZBGet uses the key as the NZB name
            url: release.download_url.clone(),  // clone: request owns the download URL
            category: None,
            idempotency_key: item.idempotency_key.clone(), // clone: request owns the duplicate key
            info_hash: release.info_hash.clone(), // clone: torrent client needs the info hash
        };
        match client.add_download(&request).await {
            Ok(client_ref) => {
                self.cas_grabbed(&item, client_ref).await?;
                self.emit("Grabbed", &item.title).await;
                Ok(true)
            }
            Err(e) => {
                self.mark_failed(&item, e.to_string()).await?;
                Ok(false)
            }
        }
    }

    async fn cas_grabbed(&self, item: &QueueItem, client_ref: String) -> Result<(), DbError> {
        let update = QueueItemUpdate {
            state: Some(DownloadState::Grabbed),
            client_ref: Some(client_ref),
            ..Default::default()
        };
        if self
            .db
            .cas_queue_state(item.id, item.revision, &update)
            .await?
            .is_none()
        {
            tracing::warn!(queue = %item.id, "cas Submitting→Grabbed lost");
        }
        Ok(())
    }

    async fn poll_item(&self, item: QueueItem) -> Result<(), DbError> {
        if !matches!(
            item.state,
            DownloadState::Grabbed | DownloadState::Downloading | DownloadState::Importing
        ) {
            return Ok(());
        }
        let Some(client_ref) = &item.client_ref else {
            return Ok(());
        };
        let Some(client) = self.client_for_item(&item) else {
            return Ok(());
        };
        let status = match client.get_status(client_ref).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "download status failed");
                return Ok(());
            }
        };
        if status.state == DownloadState::Failed {
            self.mark_failed(
                &item,
                status.error.unwrap_or_else(|| "download failed".into()),
            )
            .await?;
            return Ok(());
        }
        if status.state != DownloadState::Imported {
            if item.state == DownloadState::Grabbed {
                self.cas_state(&item, DownloadState::Downloading).await?;
            }
            return Ok(());
        }
        let Some(path) = status.output_path else {
            return Ok(());
        };
        let importing = self.ensure_importing(item).await?;
        let Some(importing) = importing else {
            return Ok(());
        };
        self.import_completed(&importing, &path).await
    }

    async fn ensure_importing(&self, item: QueueItem) -> Result<Option<QueueItem>, DbError> {
        if item.state == DownloadState::Importing {
            return Ok(Some(item));
        }
        self.cas_state(&item, DownloadState::Importing).await
    }

    async fn cas_state(
        &self,
        item: &QueueItem,
        state: DownloadState,
    ) -> Result<Option<QueueItem>, DbError> {
        self.db
            .cas_queue_state(
                item.id,
                item.revision,
                &QueueItemUpdate {
                    state: Some(state),
                    ..Default::default()
                },
            )
            .await
    }

    async fn mark_failed(&self, item: &QueueItem, error: String) -> Result<(), DbError> {
        self.emit("Failed", &error).await;
        self.db
            .update_queue_item(
                item.id,
                &QueueItemUpdate {
                    state: Some(DownloadState::Failed),
                    error: Some(error),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    async fn import_completed(&self, item: &QueueItem, path: &Path) -> Result<(), DbError> {
        let config = ImportConfig {
            mode: self.settings.import_mode,
            library_root: self.settings.library_root.clone(), // clone: ImportConfig owns paths
            naming_template: self.settings.naming_template.clone(), // clone: ImportConfig owns the template
            exclusions: vec![],
            covers_dir: self.settings.covers_dir.clone(), // clone: ImportConfig owns paths
            organise: true,
        };
        let files = list_supported(path);
        if files.is_empty() {
            return Ok(());
        }
        let before = self.library_files(item.work_id).await?;
        let mut conflict = false;
        let mut imported_format = None;
        for file in &files {
            match import_file_for_work(file, item.work_id, &config, self.db.as_ref()).await {
                Ok(TargetedImport::Imported(result)) => {
                    imported_format = Some(result.file.format);
                }
                Ok(TargetedImport::SameWorkDuplicate(_)) => {}
                Ok(TargetedImport::OtherWorkDuplicate(_)) => conflict = true,
                Err(e) => {
                    self.mark_failed(item, e.to_string()).await?;
                    return Ok(());
                }
            }
        }
        let state = if conflict {
            DownloadState::Conflict
        } else {
            DownloadState::Imported
        };
        self.db
            .update_queue_item(
                item.id,
                &QueueItemUpdate {
                    state: Some(state),
                    output_path: Some(path.display().to_string()),
                    ..Default::default()
                },
            )
            .await?;
        if state == DownloadState::Imported {
            if let Some(fmt) = imported_format {
                self.recycle_worse(&before, fmt).await;
            }
            self.emit("Imported", &item.title).await;
            self.cleanup_completed(path);
        } else {
            self.emit("Conflict", &item.title).await;
        }
        Ok(())
    }

    async fn recycle_worse(&self, before: &[LibraryFile], new_format: FileFormat) {
        let Ok(profile) = self.load_profile().await else {
            return;
        };
        let new_rank = crate::score::format_position(new_format, &profile.format_order);
        for file in before {
            if file.deleted_at.is_some() {
                continue;
            }
            let old_rank = crate::score::format_position(file.format, &profile.format_order);
            if old_rank <= new_rank {
                continue;
            }
            if let Err(e) = soft_delete(file.id, &self.settings.recycle_dir, self.db.as_ref()).await
            {
                tracing::warn!(error = %e, file = %file.id, "upgrade recycle failed");
            }
        }
    }

    async fn library_files(&self, work_id: WorkId) -> Result<Vec<LibraryFile>, DbError> {
        let editions = self
            .db
            .list_editions(
                &EditionFilter {
                    work_id: Some(work_id),
                    ..Default::default()
                },
                &Pagination {
                    page: 1,
                    per_page: 100,
                },
            )
            .await?;
        let mut files = Vec::new();
        for edition in editions.items {
            let page = self
                .db
                .list_files(
                    &FileFilter {
                        edition_id: Some(edition.id),
                        ..Default::default()
                    },
                    &Pagination {
                        page: 1,
                        per_page: 100,
                    },
                )
                .await?;
            files.extend(page.items);
        }
        Ok(files)
    }

    async fn existing_formats(&self, work_id: WorkId) -> Result<Vec<FileFormat>, DbError> {
        Ok(self
            .library_files(work_id)
            .await?
            .into_iter()
            .filter(|f| f.deleted_at.is_none())
            .map(|f| f.format)
            .collect())
    }

    fn cleanup_completed(&self, path: &Path) {
        let inside = self
            .settings
            .download_roots
            .iter()
            .any(|root| path.starts_with(root));
        if !inside || !path.is_dir() {
            return;
        }
        if let Err(e) = std::fs::remove_dir_all(path) {
            tracing::warn!(error = %e, path = %path.display(), "cleanup of completed download failed");
        }
    }

    async fn emit(&self, title: &str, body: &str) {
        let Some(notifier) = &self.notifier else {
            return;
        };
        let event = Notification {
            title: title.to_string(),
            body: body.to_string(),
        };
        if let Err(e) = notifier.notify(&event).await {
            tracing::warn!(error = %e, "notify failed");
        }
    }

    async fn load_profile(&self) -> Result<QualityProfile, DbError> {
        let id = DEFAULT_QUALITY_PROFILE_ID
            .parse()
            .map_err(|e| DbError::Internal(Box::new(std::io::Error::other(format!("{e}")))))?;
        self.db
            .get_quality_profile(id)
            .await?
            .ok_or(DbError::NotFound {
                entity: "quality_profile",
                id: DEFAULT_QUALITY_PROFILE_ID.into(),
            })
    }
}

fn matches_queue_item(
    status: &curatarr_core::traits::download_client::DownloadStatus,
    item: &QueueItem,
) -> bool {
    item.client_ref.as_deref() == Some(status.client_ref.as_str())
        || matches!(
            status.name.as_deref(),
            Some(name) if name == item.idempotency_key || name == item.title
        )
}

fn list_supported(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return if detect_format(path).is_ok() {
            vec![path.to_path_buf()]
        } else {
            vec![]
        };
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return vec![];
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && detect_format(p).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use curatarr_core::error::{ClientError, CoreError, IndexerError};
    use curatarr_core::traits::download_client::{ClientHealth, ClientType, DownloadStatus};
    use curatarr_core::traits::indexer::{IndexerHealth, IndexerProtocol};
    use curatarr_core::types::Pagination;
    use curatarr_core::types::edition::EditionFilter;
    use curatarr_core::types::enums::{ContentType, ReadStatus};
    use curatarr_core::types::file::FileFilter;
    use curatarr_core::types::work::NewWork;
    use curatarr_db::create_repository;
    use std::sync::Mutex;
    use url::Url;

    struct FakeIndexer {
        hits: Vec<Release>,
    }

    #[async_trait]
    impl Indexer for FakeIndexer {
        fn name(&self) -> &str {
            "fake"
        }
        fn protocol(&self) -> IndexerProtocol {
            IndexerProtocol::Newznab
        }
        async fn search(&self, _query: &IndexerQuery) -> Result<Vec<Release>, IndexerError> {
            Ok(self.hits.clone())
        }
        async fn health_check(&self) -> Result<IndexerHealth, IndexerError> {
            Ok(IndexerHealth {
                available: true,
                last_error: None,
            })
        }
    }

    struct FakeClient {
        added: Mutex<Vec<String>>,
        complete: Option<PathBuf>,
        history: Mutex<Vec<DownloadStatus>>,
        fail_left: Mutex<u32>,
        client_type: ClientType,
        name: &'static str,
    }

    impl FakeClient {
        fn new(complete: Option<PathBuf>) -> Arc<Self> {
            Arc::new(Self {
                added: Mutex::new(vec![]),
                complete,
                history: Mutex::new(vec![]),
                fail_left: Mutex::new(0),
                client_type: ClientType::Usenet,
                name: "fake",
            })
        }
    }

    #[async_trait]
    impl DownloadClient for FakeClient {
        fn name(&self) -> &str {
            self.name
        }
        fn client_type(&self) -> ClientType {
            self.client_type
        }
        async fn add_download(&self, request: &DownloadRequest) -> Result<String, ClientError> {
            let mut left = self.fail_left.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(curatarr_core::error::ProviderError::Request {
                    provider: self.name.into(),
                    reason: "forced failure".into(),
                }
                .into());
            }
            self.added.lock().unwrap().push(request.name.clone());
            self.history.lock().unwrap().push(DownloadStatus {
                client_ref: "1".into(),
                name: Some(request.idempotency_key.clone()),
                state: DownloadState::Downloading,
                output_path: None,
                error: None,
            });
            Ok("1".into())
        }
        async fn get_status(&self, client_ref: &str) -> Result<DownloadStatus, ClientError> {
            Ok(DownloadStatus {
                client_ref: client_ref.into(),
                name: Some(client_ref.into()),
                state: if self.complete.is_some() {
                    DownloadState::Imported
                } else {
                    DownloadState::Downloading
                },
                output_path: self.complete.clone(),
                error: None,
            })
        }
        async fn list_downloads(&self) -> Result<Vec<DownloadStatus>, ClientError> {
            Ok(self.history.lock().unwrap().clone())
        }
        async fn health_check(&self) -> Result<ClientHealth, ClientError> {
            Ok(ClientHealth {
                available: true,
                version: None,
                last_error: None,
            })
        }
    }

    struct RecordingNotifier {
        events: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl Notifier for RecordingNotifier {
        fn name(&self) -> &str {
            "rec"
        }
        async fn notify(&self, event: &Notification) -> Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push((event.title.clone(), event.body.clone()));
            Ok(())
        }
        async fn test(&self) -> Result<(), CoreError> {
            Ok(())
        }
    }

    fn book(title: &str, monitored: bool) -> NewWork {
        NewWork {
            title: title.into(),
            sort_title: title.into(),
            original_language: None,
            original_pub_date: None,
            description: None,
            description_html: None,
            content_type: ContentType::Book,
            age_rating: None,
            content_warnings: vec![],
            read_status: ReadStatus::Unread,
            monitored,
        }
    }

    fn epub_release() -> Release {
        Release {
            title: "Dune EPUB".into(),
            guid: "g".into(),
            indexer: "x".into(),
            download_url: Url::parse("http://x/d.nzb").unwrap(),
            size_bytes: 10,
            protocol: IndexerProtocol::Newznab,
            info_hash: None,
        }
    }

    fn write_pdf(path: &Path, salt: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("%PDF-1.4\n% {salt}\n")).unwrap();
    }

    async fn files_for_work(
        db: &dyn Repository,
        work_id: WorkId,
    ) -> Vec<curatarr_core::types::file::LibraryFile> {
        let editions = db
            .list_editions(
                &EditionFilter {
                    work_id: Some(work_id),
                    ..Default::default()
                },
                &Pagination {
                    page: 1,
                    per_page: 50,
                },
            )
            .await
            .unwrap();
        let mut files = Vec::new();
        for edition in editions.items {
            let page = db
                .list_files(
                    &FileFilter {
                        edition_id: Some(edition.id),
                        ..Default::default()
                    },
                    &Pagination {
                        page: 1,
                        per_page: 50,
                    },
                )
                .await
                .unwrap();
            files.extend(page.items);
        }
        files
    }

    fn settings(dir: &Path, complete: &Path) -> AcquireSettings {
        AcquireSettings {
            library_root: dir.join("lib"),
            covers_dir: dir.join("covers"),
            recycle_dir: dir.join("recycle"),
            naming_template: "{Title}.{Extension}".into(),
            import_mode: ImportMode::Copy,
            download_roots: vec![complete.to_path_buf()],
        }
    }

    #[tokio::test]
    async fn grab_wanted_work_calls_downloader() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let created = db.create_work(&book("Dune", true)).await.unwrap();
        let notifier = Arc::new(RecordingNotifier {
            events: Mutex::new(vec![]),
        });
        let client = FakeClient::new(None);
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer {
                hits: vec![epub_release()],
            }),
            client.clone(),
            settings(dir.path(), dir.path()),
        )
        .with_notifier(notifier.clone());
        let n = svc.search_wanted().await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(client.added.lock().unwrap().len(), 1);
        assert!(
            db.active_queue_for_work(created.id)
                .await
                .unwrap()
                .is_some()
        );
        let events = notifier.events.lock().unwrap().clone();
        assert!(events.iter().any(|(t, _)| t == "Grabbed"));
    }

    #[tokio::test]
    async fn poll_imports_completed_file_onto_queued_work() {
        let dir = tempfile::tempdir().unwrap();
        let complete = dir.path().join("complete");
        let pdf = complete.join("Other Title.pdf");
        write_pdf(&pdf, "import-a");
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Target", true)).await.unwrap();
        let client = FakeClient::new(Some(complete.clone()));
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer { hits: vec![] }),
            client,
            settings(dir.path(), &complete),
        );
        db.create_queue_item(&NewQueueItem {
            work_id: work.id,
            state: DownloadState::Grabbed,
            protocol: None,
            indexer: None,
            title: "Target".into(),
            guid: None,
            download_url: None,
            client: Some("fake".into()),
            idempotency_key: "q-1".into(),
        })
        .await
        .unwrap();
        db.update_queue_item(
            db.active_queue_for_work(work.id).await.unwrap().unwrap().id,
            &QueueItemUpdate {
                client_ref: Some("1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        svc.poll_queue().await.unwrap();

        let files = files_for_work(db.as_ref(), work.id).await;
        assert_eq!(files.len(), 1);
        let edition = db.get_edition(files[0].edition_id).await.unwrap().unwrap();
        assert_eq!(edition.work_id, work.id);
        assert!(db.list_wanted_work_ids().await.unwrap().is_empty());
        let queue = db.list_queue().await.unwrap();
        assert_eq!(queue[0].state, DownloadState::Imported);
    }

    #[tokio::test]
    async fn hash_duplicate_on_other_work_is_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let complete = dir.path().join("complete");
        let pdf = complete.join("Shared.pdf");
        write_pdf(&pdf, "shared");
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work_a = db.create_work(&book("A", true)).await.unwrap();
        let work_b = db.create_work(&book("B", false)).await.unwrap();
        let config = ImportConfig {
            mode: ImportMode::Copy,
            library_root: dir.path().join("lib"),
            naming_template: "{Title}.{Extension}".into(),
            exclusions: vec![],
            covers_dir: dir.path().join("covers"),
            organise: true,
        };
        import_file_for_work(&pdf, work_b.id, &config, db.as_ref())
            .await
            .unwrap();

        let client = FakeClient::new(Some(complete.clone()));
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer { hits: vec![] }),
            client,
            settings(dir.path(), &complete),
        );
        let item = db
            .create_queue_item(&NewQueueItem {
                work_id: work_a.id,
                state: DownloadState::Grabbed,
                protocol: None,
                indexer: None,
                title: "A".into(),
                guid: None,
                download_url: None,
                client: Some("fake".into()),
                idempotency_key: "q-a".into(),
            })
            .await
            .unwrap();
        db.update_queue_item(
            item.id,
            &QueueItemUpdate {
                client_ref: Some("1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        svc.poll_queue().await.unwrap();

        let queue = db.get_queue_item(item.id).await.unwrap().unwrap();
        assert_eq!(queue.state, DownloadState::Conflict);
        assert!(
            db.list_wanted_work_ids()
                .await
                .unwrap()
                .contains(&work_a.id)
        );
        assert!(files_for_work(db.as_ref(), work_a.id).await.is_empty());
    }

    #[tokio::test]
    async fn hash_duplicate_on_same_work_is_imported() {
        let dir = tempfile::tempdir().unwrap();
        let complete = dir.path().join("complete");
        let pdf = complete.join("Same.pdf");
        write_pdf(&pdf, "same-work");
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("A", true)).await.unwrap();
        let config = ImportConfig {
            mode: ImportMode::Copy,
            library_root: dir.path().join("lib"),
            naming_template: "{Title}.{Extension}".into(),
            exclusions: vec![],
            covers_dir: dir.path().join("covers"),
            organise: true,
        };
        import_file_for_work(&pdf, work.id, &config, db.as_ref())
            .await
            .unwrap();

        let client = FakeClient::new(Some(complete.clone()));
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer { hits: vec![] }),
            client,
            settings(dir.path(), &complete),
        );
        let item = db
            .create_queue_item(&NewQueueItem {
                work_id: work.id,
                state: DownloadState::Grabbed,
                protocol: None,
                indexer: None,
                title: "A".into(),
                guid: None,
                download_url: None,
                client: Some("fake".into()),
                idempotency_key: "q-same".into(),
            })
            .await
            .unwrap();
        db.update_queue_item(
            item.id,
            &QueueItemUpdate {
                client_ref: Some("1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        svc.poll_queue().await.unwrap();
        assert_eq!(
            db.get_queue_item(item.id).await.unwrap().unwrap().state,
            DownloadState::Imported
        );
    }

    #[tokio::test]
    async fn reconcile_repairs_submitting_from_client_history() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let item = db
            .create_queue_item(&NewQueueItem {
                work_id: work.id,
                state: DownloadState::Submitting,
                protocol: None,
                indexer: None,
                title: "Dune EPUB".into(),
                guid: None,
                download_url: None,
                client: Some("fake".into()),
                idempotency_key: "q-crash".into(),
            })
            .await
            .unwrap();
        let client = FakeClient::new(None);
        client.history.lock().unwrap().push(DownloadStatus {
            client_ref: "99".into(),
            name: Some("q-crash".into()),
            state: DownloadState::Downloading,
            output_path: None,
            error: None,
        });
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer { hits: vec![] }),
            client,
            settings(dir.path(), dir.path()),
        );
        svc.reconcile().await.unwrap();
        let repaired = db.get_queue_item(item.id).await.unwrap().unwrap();
        assert_eq!(repaired.state, DownloadState::Grabbed);
        assert_eq!(repaired.client_ref.as_deref(), Some("99"));
    }

    #[tokio::test]
    async fn grab_guid_picks_named_release() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let mut pdf = epub_release();
        pdf.title = "Dune PDF".into();
        pdf.guid = "pdf".into();
        let client = FakeClient::new(None);
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer {
                hits: vec![epub_release(), pdf],
            }),
            client.clone(),
            settings(dir.path(), dir.path()),
        );
        assert!(svc.grab_guid(work.id, "pdf").await.unwrap());
        let queue = db.active_queue_for_work(work.id).await.unwrap().unwrap();
        assert_eq!(queue.guid.as_deref(), Some("pdf"));
        assert_eq!(client.added.lock().unwrap().len(), 1);
    }

    fn import_cfg(dir: &Path) -> ImportConfig {
        ImportConfig {
            mode: ImportMode::Copy,
            library_root: dir.join("lib"),
            naming_template: "{Title}.{Extension}".into(),
            exclusions: vec![],
            covers_dir: dir.join("covers"),
            organise: true,
        }
    }

    fn write_epub(path: &Path, salt: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = b"PK\x03\x04".to_vec();
        bytes.extend(salt.as_bytes());
        std::fs::write(path, bytes).unwrap();
    }

    fn pdf_release() -> Release {
        let mut r = epub_release();
        r.title = "Dune PDF".into();
        r.guid = "pdf".into();
        r
    }

    #[tokio::test]
    async fn pdf_on_disk_grabs_epub_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let pdf = dir.path().join("inbox/Dune.pdf");
        write_pdf(&pdf, "on-disk");
        import_file_for_work(&pdf, work.id, &import_cfg(dir.path()), db.as_ref())
            .await
            .unwrap();
        let client = FakeClient::new(None);
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer {
                hits: vec![epub_release()],
            }),
            client.clone(),
            settings(dir.path(), dir.path()),
        );
        assert_eq!(svc.search_wanted().await.unwrap(), 1);
        assert_eq!(client.added.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn epub_on_disk_skips_pdf_release() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let epub = dir.path().join("inbox/Dune.epub");
        write_epub(&epub, "on-disk");
        import_file_for_work(&epub, work.id, &import_cfg(dir.path()), db.as_ref())
            .await
            .unwrap();
        let client = FakeClient::new(None);
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer {
                hits: vec![pdf_release()],
            }),
            client.clone(),
            settings(dir.path(), dir.path()),
        );
        assert_eq!(svc.search_wanted().await.unwrap(), 0);
        assert!(client.added.lock().unwrap().is_empty());
        assert!(db.active_queue_for_work(work.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upgrade_import_recycles_worse_file() {
        let dir = tempfile::tempdir().unwrap();
        let complete = dir.path().join("complete");
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let pdf = dir.path().join("inbox/Dune.pdf");
        write_pdf(&pdf, "old-pdf");
        import_file_for_work(&pdf, work.id, &import_cfg(dir.path()), db.as_ref())
            .await
            .unwrap();
        let old_files = files_for_work(db.as_ref(), work.id).await;
        assert_eq!(old_files.len(), 1);
        write_epub(&complete.join("Better.epub"), "new-epub");
        let client = FakeClient::new(Some(complete.clone()));
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer { hits: vec![] }),
            client,
            settings(dir.path(), &complete),
        );
        let item = db
            .create_queue_item(&NewQueueItem {
                work_id: work.id,
                state: DownloadState::Grabbed,
                protocol: None,
                indexer: None,
                title: "Dune EPUB".into(),
                guid: None,
                download_url: None,
                client: Some("fake".into()),
                idempotency_key: "q-up".into(),
            })
            .await
            .unwrap();
        db.update_queue_item(
            item.id,
            &QueueItemUpdate {
                client_ref: Some("1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        svc.poll_queue().await.unwrap();
        let remaining = files_for_work(db.as_ref(), work.id).await;
        assert!(
            remaining.iter().any(|f| f.format == FileFormat::Epub),
            "{remaining:?}"
        );
        let old = db.get_file(old_files[0].id).await.unwrap().unwrap();
        assert!(old.deleted_at.is_some());
    }

    #[tokio::test]
    async fn first_release_failure_retries_next() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let client = FakeClient::new(None);
        *client.fail_left.lock().unwrap() = 1;
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer {
                hits: vec![epub_release(), pdf_release()],
            }),
            client.clone(),
            settings(dir.path(), dir.path()),
        );
        assert!(svc.grab_work(work.id).await.unwrap());
        assert_eq!(client.added.lock().unwrap().len(), 1);
        let queue = db.active_queue_for_work(work.id).await.unwrap().unwrap();
        assert_eq!(queue.guid.as_deref(), Some("pdf"));
        let all = db.list_queue().await.unwrap();
        assert!(all.iter().any(|i| i.state == DownloadState::Failed));
    }

    #[tokio::test]
    async fn torrent_release_skipped_without_qbit() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        db.create_work(&book("Dune", true)).await.unwrap();
        let mut torrent = epub_release();
        torrent.protocol = IndexerProtocol::Torznab;
        torrent.info_hash = Some("abc".into());
        let client = FakeClient::new(None);
        let svc = AcquireService::new(
            db,
            Arc::new(FakeIndexer {
                hits: vec![torrent],
            }),
            client.clone(),
            settings(dir.path(), dir.path()),
        );
        assert_eq!(svc.search_wanted().await.unwrap(), 0);
        assert!(client.added.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn torrent_release_grabs_when_qbit_configured() {
        let dir = tempfile::tempdir().unwrap();
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db.create_work(&book("Dune", true)).await.unwrap();
        let mut torrent = epub_release();
        torrent.protocol = IndexerProtocol::Torznab;
        torrent.guid = "t1".into();
        torrent.info_hash = Some("abc".into());
        let usenet = FakeClient::new(None);
        let qbit = Arc::new(FakeClient {
            added: Mutex::new(vec![]),
            complete: None,
            history: Mutex::new(vec![]),
            fail_left: Mutex::new(0),
            client_type: ClientType::Torrent,
            name: "qbittorrent",
        });
        let svc = AcquireService::new(
            db.clone(),
            Arc::new(FakeIndexer {
                hits: vec![torrent],
            }),
            usenet,
            settings(dir.path(), dir.path()),
        )
        .with_torrent(qbit.clone());
        assert!(svc.grab_work(work.id).await.unwrap());
        assert_eq!(qbit.added.lock().unwrap().len(), 1);
        let queue = db.active_queue_for_work(work.id).await.unwrap().unwrap();
        assert_eq!(queue.client.as_deref(), Some("qbittorrent"));
    }
}
