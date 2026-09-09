//! Recycle bin: soft delete moves a file aside and marks the record deleted; restore reverses
//! it; purge removes both permanently. Cleanup purges entries older than the retention period.

use chrono::{Duration, Utc};
use curatarr_core::error::{DbError, ScannerError};
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::file::{LibraryFile, LibraryFileUpdate};
use curatarr_core::types::id::FileId;
use curatarr_core::types::recycle::{NewRecycleEntry, RecycleEntry};
use std::path::{Path, PathBuf};

use crate::fsops::{io_err, move_file};

fn not_found(id: FileId) -> ScannerError {
    ScannerError::Database(DbError::NotFound {
        entity: "file",
        id: id.to_string(),
    })
}

/// Move the file into `{recycle_dir}/{file_id}/{filename}` and mark the record deleted.
pub async fn soft_delete(
    file_id: FileId,
    recycle_dir: &Path,
    db: &dyn Repository,
) -> Result<RecycleEntry, ScannerError> {
    let file = db
        .get_file(file_id)
        .await?
        .ok_or_else(|| not_found(file_id))?;
    if file.deleted_at.is_some() || db.get_recycle_entry_for_file(file_id).await?.is_some() {
        return Err(ScannerError::AlreadyRecycled(file_id.to_string()));
    }

    let original = PathBuf::from(&file.path);
    let file_name = original
        .file_name()
        .map(|n| n.to_os_string())
        .ok_or_else(|| {
            io_err(
                &original,
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name"),
            )
        })?;
    let recycle_path = recycle_dir.join(file_id.to_string()).join(file_name);

    move_file(&original, &recycle_path).await?;

    let entry = match db
        .create_recycle_entry(&NewRecycleEntry {
            original_file_id: file_id,
            original_path: file.path.clone(), // clone: path is stored on the entry and kept on the file row
            recycle_path: recycle_path.to_string_lossy().into_owned(),
        })
        .await
    {
        Ok(entry) => entry,
        Err(e) => {
            // Put the file back so disk and database stay consistent.
            if let Err(undo) = move_file(&recycle_path, &original).await {
                tracing::error!(error = %undo, "failed to undo recycle move");
            }
            return Err(e.into());
        }
    };

    db.update_file(
        file_id,
        &LibraryFileUpdate {
            path: None,
            deleted_at: Some(Some(Utc::now())),
        },
    )
    .await?;

    Ok(entry)
}

/// Move a recycled file back to its original path and clear the deleted marker.
pub async fn restore(file_id: FileId, db: &dyn Repository) -> Result<LibraryFile, ScannerError> {
    let entry = db
        .get_recycle_entry_for_file(file_id)
        .await?
        .ok_or_else(|| ScannerError::NotRecycled(file_id.to_string()))?;

    move_file(
        Path::new(&entry.recycle_path),
        Path::new(&entry.original_path),
    )
    .await?;
    remove_empty_parent(Path::new(&entry.recycle_path)).await;

    let restored = db
        .update_file(
            file_id,
            &LibraryFileUpdate {
                path: Some(entry.original_path.clone()), // clone: entry is still needed below
                deleted_at: Some(None),
            },
        )
        .await?;
    db.delete_recycle_entry(entry.id).await?;
    Ok(restored)
}

/// Permanently remove a recycled file from disk and the database.
pub async fn purge(file_id: FileId, db: &dyn Repository) -> Result<(), ScannerError> {
    let entry = db
        .get_recycle_entry_for_file(file_id)
        .await?
        .ok_or_else(|| ScannerError::NotRecycled(file_id.to_string()))?;

    let recycle_path = Path::new(&entry.recycle_path);
    match tokio::fs::remove_file(recycle_path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_err(recycle_path, e)),
    }
    remove_empty_parent(recycle_path).await;

    db.delete_recycle_entry(entry.id).await?;
    db.delete_file(file_id).await?;
    Ok(())
}

/// Purge every recycle entry older than `retention_days`. Returns the number purged.
pub async fn cleanup_recycle_bin(
    retention_days: u32,
    db: &dyn Repository,
) -> Result<u64, ScannerError> {
    let cutoff = Utc::now() - Duration::days(i64::from(retention_days));
    let mut purged = 0u64;
    for entry in db.list_recycle_entries().await? {
        if entry.deleted_at < cutoff {
            purge(entry.original_file_id, db).await?;
            purged += 1;
        }
    }
    Ok(purged)
}

async fn remove_empty_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        // Only succeeds when the directory is empty; that's the intent.
        let _ = tokio::fs::remove_dir(parent).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_core::types::edition::NewEdition;
    use curatarr_core::types::enums::{ContentType, FileFormat, ReadStatus};
    use curatarr_core::types::file::NewLibraryFile;
    use curatarr_core::types::work::NewWork;
    use curatarr_db::create_repository;
    use std::sync::Arc;

    async fn seeded(dir: &Path) -> (Arc<dyn Repository>, LibraryFile) {
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db
            .create_work(&NewWork {
                title: "Dune".into(),
                sort_title: "Dune".into(),
                original_language: None,
                original_pub_date: None,
                description: None,
                description_html: None,
                content_type: ContentType::Book,
                age_rating: None,
                content_warnings: vec![],
                read_status: ReadStatus::Unread,
                monitored: false,
            })
            .await
            .unwrap();
        let edition = db
            .create_edition(&NewEdition {
                work_id: work.id,
                isbn13: None,
                isbn10: None,
                asin: None,
                publisher_id: None,
                imprint: None,
                publication_date: None,
                edition_number: None,
                format: FileFormat::Epub,
                page_count: None,
                word_count: None,
                language: None,
                translator: None,
            })
            .await
            .unwrap();
        let path = dir.join("library/Dune.epub");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"book bytes").unwrap();
        let file = db
            .create_file(&NewLibraryFile {
                edition_id: edition.id,
                path: path.to_string_lossy().into_owned(),
                format: FileFormat::Epub,
                size_bytes: 10,
                sha256: "abc".into(),
            })
            .await
            .unwrap();
        (db, file)
    }

    #[tokio::test]
    async fn soft_delete_moves_file_and_marks_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let (db, file) = seeded(dir.path()).await;
        let recycle_dir = dir.path().join("recycle");

        let entry = soft_delete(file.id, &recycle_dir, db.as_ref())
            .await
            .unwrap();
        assert!(!Path::new(&file.path).exists());
        assert!(Path::new(&entry.recycle_path).exists());
        assert!(
            entry
                .recycle_path
                .starts_with(recycle_dir.to_string_lossy().as_ref())
        );

        let row = db.get_file(file.id).await.unwrap().unwrap();
        assert!(row.deleted_at.is_some());
        assert!(db.find_file_by_hash("abc").await.unwrap().is_none());

        let again = soft_delete(file.id, &recycle_dir, db.as_ref()).await;
        assert!(matches!(again, Err(ScannerError::AlreadyRecycled(_))));
    }

    #[tokio::test]
    async fn restore_returns_file_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let (db, file) = seeded(dir.path()).await;
        let recycle_dir = dir.path().join("recycle");

        soft_delete(file.id, &recycle_dir, db.as_ref())
            .await
            .unwrap();
        let restored = restore(file.id, db.as_ref()).await.unwrap();

        assert_eq!(restored.path, file.path);
        assert!(restored.deleted_at.is_none());
        assert_eq!(std::fs::read(&file.path).unwrap(), b"book bytes");
        assert!(
            db.get_recycle_entry_for_file(file.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!recycle_dir.join(file.id.to_string()).exists());

        let not_recycled = restore(file.id, db.as_ref()).await;
        assert!(matches!(not_recycled, Err(ScannerError::NotRecycled(_))));
    }

    #[tokio::test]
    async fn cleanup_purges_only_expired_entries() {
        let dir = tempfile::tempdir().unwrap();
        let (db, file) = seeded(dir.path()).await;
        let recycle_dir = dir.path().join("recycle");
        let entry = soft_delete(file.id, &recycle_dir, db.as_ref())
            .await
            .unwrap();

        // Fresh entry survives a 30-day retention.
        assert_eq!(cleanup_recycle_bin(30, db.as_ref()).await.unwrap(), 0);
        assert!(Path::new(&entry.recycle_path).exists());

        // Zero retention purges everything deleted before "now".
        assert_eq!(cleanup_recycle_bin(0, db.as_ref()).await.unwrap(), 1);
        assert!(!Path::new(&entry.recycle_path).exists());
        assert!(db.get_file(file.id).await.unwrap().is_none());
        assert!(db.list_recycle_entries().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn soft_delete_unknown_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _) = seeded(dir.path()).await;
        let result = soft_delete(FileId::new(), dir.path(), db.as_ref()).await;
        assert!(matches!(
            result,
            Err(ScannerError::Database(DbError::NotFound { .. }))
        ));
    }
}
