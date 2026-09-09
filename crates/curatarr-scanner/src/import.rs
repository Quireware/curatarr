//! Import pipeline: turn a file on disk into Work / Edition / Author / Series / File records.
//!
//! Steps per file: detect format → hash → duplicate check → extract metadata → plan destination
//! → transfer (copy / move / hardlink) → persist entities → process cover.
//! If persisting fails after a transfer, the transfer is undone; if persisting fails after new
//! entities were created, those entities are removed again.

use curatarr_core::error::ScannerError;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::author::{Author, AuthorFilter, NewAuthor};
use curatarr_core::types::edition::{Edition, EditionFilter, EditionUpdate, NewEdition};
use curatarr_core::types::enums::{AuthorRole, ContentType, FileFormat, ImportMode, ReadStatus};
use curatarr_core::types::enums::{ReadingOrder, SeriesType};
use curatarr_core::types::file::{LibraryFile, NewLibraryFile};
use curatarr_core::types::id::{EditionId, WorkId};
use curatarr_core::types::identifiers::{Isbn10, Isbn13};
use curatarr_core::types::publisher::{NewPublisher, PublisherFilter};
use curatarr_core::types::series::{NewSeries, NewSeriesEntry, Series, SeriesFilter};
use curatarr_core::types::work::{NewWork, Work, WorkFilter};
use std::path::{Path, PathBuf};

use crate::cover::process_cover;
use crate::extractors::{ExtractedMetadata, default_content_type, extract_metadata};
use crate::format::detect_format;
use crate::fsops::{io_err, move_file};
use crate::hash::hash_file;
use crate::naming::{NamingContext, apply_template, sort_name_for};
use crate::scanner::scan_directory;

/// Page size used when walking whole tables for matching.
const MATCH_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportConfig {
    pub mode: ImportMode,
    /// Destination library root when `organise` is set.
    pub library_root: PathBuf,
    pub naming_template: String,
    pub exclusions: Vec<String>,
    /// Content-addressed cover storage directory.
    pub covers_dir: PathBuf,
    /// `true`: rename/move files into `library_root` per the naming template.
    /// `false`: catalogue files where they are (in-place scan of a root folder).
    pub organise: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub work: Work,
    pub edition: Edition,
    pub file: LibraryFile,
    pub authors: Vec<Author>,
    pub series: Option<Series>,
    pub is_new_work: bool,
    pub is_new_edition: bool,
    /// Content-addressed directory holding the processed cover sizes, if a cover was found.
    pub cover_dir: Option<PathBuf>,
    /// Metadata extraction problem; the record was created from filename-derived metadata.
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportOutcome {
    Imported(Box<ImportResult>),
    /// The file's content already exists in the library; nothing was changed.
    Duplicate {
        source: PathBuf,
        existing: LibraryFile,
    },
}

/// Result of attaching a file to a specific work (acquisition import).
#[derive(Debug, Clone, PartialEq)]
pub enum TargetedImport {
    Imported(Box<ImportResult>),
    SameWorkDuplicate(LibraryFile),
    OtherWorkDuplicate(LibraryFile),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    pub imported: Vec<ImportResult>,
    pub duplicates: Vec<(PathBuf, LibraryFile)>,
    pub failed: Vec<ImportFailure>,
}

impl ImportReport {
    pub fn total(&self) -> usize {
        self.imported.len() + self.duplicates.len() + self.failed.len()
    }
}

/// Progress events emitted while importing a directory.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportEvent {
    Discovered { total: usize },
    Imported { path: PathBuf },
    Duplicate { path: PathBuf },
    Failed { path: PathBuf, error: String },
}

pub async fn import_directory(
    root: &Path,
    config: &ImportConfig,
    db: &dyn Repository,
) -> Result<ImportReport, ScannerError> {
    import_directory_with_progress(root, config, db, |_| {}).await
}

pub async fn import_directory_with_progress(
    root: &Path,
    config: &ImportConfig,
    db: &dyn Repository,
    mut on_event: impl FnMut(&ImportEvent) + Send,
) -> Result<ImportReport, ScannerError> {
    let discovered = scan_directory(root, &config.exclusions)?;
    on_event(&ImportEvent::Discovered {
        total: discovered.len(),
    });

    let mut report = ImportReport::default();
    for found in discovered {
        match import_file(&found.path, config, db).await {
            Ok(ImportOutcome::Imported(result)) => {
                on_event(&ImportEvent::Imported {
                    path: found.path, // moved: DiscoveredFile is consumed here
                });
                report.imported.push(*result);
            }
            Ok(ImportOutcome::Duplicate { source, existing }) => {
                on_event(&ImportEvent::Duplicate {
                    path: source.clone(), // clone: path is reported and also stored
                });
                report.duplicates.push((source, existing));
            }
            Err(e) => {
                let error = e.to_string();
                tracing::warn!(path = %found.path.display(), %error, "import failed");
                on_event(&ImportEvent::Failed {
                    path: found.path.clone(), // clone: path is reported and also stored
                    error: error.clone(),     // clone: message is reported and also stored
                });
                report.failed.push(ImportFailure {
                    path: found.path,
                    error,
                });
            }
        }
    }
    Ok(report)
}

pub async fn import_file_for_work(
    path: &Path,
    work_id: WorkId,
    config: &ImportConfig,
    db: &dyn Repository,
) -> Result<TargetedImport, ScannerError> {
    let format = detect_format(path)?;
    let sha256 = hash_file(path).await?;
    if let Some(existing) = db.find_file_by_hash(&sha256).await? {
        let edition = db
            .get_edition(existing.edition_id)
            .await?
            .ok_or(ScannerError::Database(
                curatarr_core::error::DbError::NotFound {
                    entity: "edition",
                    id: existing.edition_id.to_string(),
                },
            ))?;
        if edition.work_id == work_id {
            return Ok(TargetedImport::SameWorkDuplicate(existing));
        }
        return Ok(TargetedImport::OtherWorkDuplicate(existing));
    }

    let meta = extract_metadata(path, format);
    let title = meta.title_or_stem(path);
    let dest = if config.organise {
        plan_destination(config, &meta, &title, format, path)?
    } else {
        path.to_path_buf()
    };
    let transferred = dest != path;
    if transferred {
        transfer(path, &dest, config.mode).await?;
    }
    let size_bytes = tokio::fs::metadata(&dest)
        .await
        .map_err(|e| io_err(&dest, e))?
        .len();
    let input = PersistInput {
        meta: &meta,
        title,
        format,
        dest: &dest,
        size_bytes,
        sha256,
        target_work: Some(work_id),
    };
    match persist(db, config, input).await {
        Ok(result) => Ok(TargetedImport::Imported(Box::new(result))),
        Err(e) => {
            if transferred {
                undo_transfer(path, &dest, config.mode).await;
            }
            Err(e)
        }
    }
}

pub async fn import_file(
    path: &Path,
    config: &ImportConfig,
    db: &dyn Repository,
) -> Result<ImportOutcome, ScannerError> {
    let format = detect_format(path)?;
    let sha256 = hash_file(path).await?;

    if let Some(existing) = db.find_file_by_hash(&sha256).await? {
        return Ok(ImportOutcome::Duplicate {
            source: path.to_path_buf(),
            existing,
        });
    }

    let meta = extract_metadata(path, format);
    let title = meta.title_or_stem(path);

    let dest = if config.organise {
        plan_destination(config, &meta, &title, format, path)?
    } else {
        path.to_path_buf()
    };

    let transferred = dest != path;
    if transferred {
        transfer(path, &dest, config.mode).await?;
    }

    let size_bytes = tokio::fs::metadata(&dest)
        .await
        .map_err(|e| io_err(&dest, e))?
        .len();

    let input = PersistInput {
        meta: &meta,
        title,
        format,
        dest: &dest,
        size_bytes,
        sha256,
        target_work: None,
    };
    match persist(db, config, input).await {
        Ok(result) => Ok(ImportOutcome::Imported(Box::new(result))),
        Err(e) => {
            if transferred {
                undo_transfer(path, &dest, config.mode).await;
            }
            Err(e)
        }
    }
}

fn plan_destination(
    config: &ImportConfig,
    meta: &ExtractedMetadata,
    title: &str,
    format: FileFormat,
    source: &Path,
) -> Result<PathBuf, ScannerError> {
    let author = meta.authors.first().cloned(); // clone: naming context owns its values
    let ctx = NamingContext {
        title: Some(title.to_string()),
        author_sort: author.as_deref().map(sort_name_for),
        author,
        series: meta.series.clone(), // clone: naming context owns its values
        series_position: meta.series_position,
        year: meta.year,
        publisher: meta.publisher.clone(), // clone: naming context owns its values
        language: meta.language.clone(),   // clone: naming context owns its values
        isbn: meta.isbn.clone(),           // clone: naming context owns its values
        extension: format.extension().to_string(),
    };
    let relative = apply_template(&config.naming_template, &ctx)?;
    let candidate = config.library_root.join(relative);
    Ok(unique_destination(candidate, source))
}

/// If `candidate` already exists (and is not the source itself), append ` (n)` before the
/// extension until a free name is found.
fn unique_destination(candidate: PathBuf, source: &Path) -> PathBuf {
    if !candidate.exists() || candidate == source {
        return candidate;
    }
    let stem = candidate
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = candidate
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = candidate
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    (2..)
        .map(|n| parent.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

async fn transfer(source: &Path, dest: &Path, mode: ImportMode) -> Result<(), ScannerError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| io_err(parent, e))?;
    }
    if tokio::fs::try_exists(dest).await.unwrap_or(false) {
        return Err(io_err(
            dest,
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "destination already exists",
            ),
        ));
    }
    match mode {
        ImportMode::Copy => {
            tokio::fs::copy(source, dest)
                .await
                .map_err(|e| io_err(dest, e))?;
        }
        ImportMode::Move => move_file(source, dest).await?,
        ImportMode::Hardlink => {
            tokio::fs::hard_link(source, dest)
                .await
                .map_err(|e| io_err(dest, e))?;
        }
    }
    Ok(())
}

async fn undo_transfer(source: &Path, dest: &Path, mode: ImportMode) {
    let result = match mode {
        ImportMode::Copy | ImportMode::Hardlink => tokio::fs::remove_file(dest).await,
        ImportMode::Move => match tokio::fs::rename(dest, source).await {
            Ok(()) => Ok(()),
            Err(_) => match tokio::fs::copy(dest, source).await {
                Ok(_) => tokio::fs::remove_file(dest).await,
                Err(e) => Err(e),
            },
        },
    };
    if let Err(e) = result {
        tracing::error!(
            dest = %dest.display(),
            source = %source.display(),
            error = %e,
            "failed to undo file transfer after import error"
        );
    }
}

/// Entities created during one import, removed again if a later step fails.
#[derive(Default)]
struct Created {
    work: Option<WorkId>,
    edition: Option<EditionId>,
}

impl Created {
    async fn rollback(self, db: &dyn Repository) {
        if let Some(work_id) = self.work {
            // cascade removes the edition and any file rows
            if let Err(e) = db.delete_work(work_id).await {
                tracing::error!(%work_id, error = %e, "rollback of new work failed");
            }
        } else if let Some(edition_id) = self.edition {
            if let Err(e) = db.delete_edition(edition_id).await {
                tracing::error!(%edition_id, error = %e, "rollback of new edition failed");
            }
        }
    }
}

/// Everything `persist` needs about the file being imported.
struct PersistInput<'a> {
    meta: &'a ExtractedMetadata,
    title: String,
    format: FileFormat,
    dest: &'a Path,
    size_bytes: u64,
    sha256: String,
    target_work: Option<WorkId>,
}

async fn persist(
    db: &dyn Repository,
    config: &ImportConfig,
    input: PersistInput<'_>,
) -> Result<ImportResult, ScannerError> {
    let mut created = Created::default();
    match persist_inner(db, config, input, &mut created).await {
        Ok(result) => Ok(result),
        Err(e) => {
            created.rollback(db).await;
            Err(e)
        }
    }
}

async fn persist_inner(
    db: &dyn Repository,
    config: &ImportConfig,
    input: PersistInput<'_>,
    created: &mut Created,
) -> Result<ImportResult, ScannerError> {
    let PersistInput {
        meta,
        title,
        format,
        dest,
        size_bytes,
        sha256,
        target_work,
    } = input;
    let content_type = meta
        .content_type
        .unwrap_or_else(|| default_content_type(format));

    let (work, is_new_work) = if let Some(id) = target_work {
        let work = db.get_work(id).await?.ok_or(ScannerError::Database(
            curatarr_core::error::DbError::NotFound {
                entity: "work",
                id: id.to_string(),
            },
        ))?;
        (work, false)
    } else {
        find_or_create_work(db, &title, &meta.authors, content_type).await?
    };
    if is_new_work {
        created.work = Some(work.id);
    }

    let mut authors = Vec::new();
    for name in &meta.authors {
        let author = find_or_create_author(db, name).await?;
        db.link_work_author(work.id, author.id, AuthorRole::Author)
            .await?;
        authors.push(author);
    }
    for name in &meta.illustrators {
        let author = find_or_create_author(db, name).await?;
        db.link_work_author(work.id, author.id, AuthorRole::Illustrator)
            .await?;
        if !authors.iter().any(|a| a.id == author.id) {
            authors.push(author);
        }
    }

    let publisher_id = match &meta.publisher {
        Some(name) => Some(find_or_create_publisher(db, name).await?),
        None => None,
    };

    let (edition, is_new_edition) =
        find_or_create_edition(db, &work, meta, format, publisher_id).await?;
    if is_new_edition {
        created.edition = Some(edition.id);
    }

    let series = match &meta.series {
        Some(series_title) => {
            Some(attach_series(db, work.id, series_title, meta.series_position).await?)
        }
        None => None,
    };

    let file = db
        .create_file(&NewLibraryFile {
            edition_id: edition.id,
            path: dest.to_string_lossy().into_owned(),
            format,
            size_bytes,
            sha256,
        })
        .await?;

    let (edition, cover_dir) = match (&meta.cover, &edition.cover_path) {
        (Some(bytes), None) => {
            let bytes = bytes.clone(); // clone: cover bytes move into the blocking task
            let covers_dir = config.covers_dir.clone(); // clone: path moves into the blocking task
            let processed = tokio::task::spawn_blocking(move || process_cover(&bytes, &covers_dir))
                .await
                .map_err(|e| ScannerError::ExtractionFailed {
                    path: dest.to_path_buf(),
                    reason: format!("cover task panicked: {e}"),
                });
            match processed {
                Ok(Ok(paths)) => {
                    let dir = paths
                        .original
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or(paths.original);
                    let updated = db
                        .update_edition(
                            edition.id,
                            &EditionUpdate {
                                cover_path: Some(Some(dir.to_string_lossy().into_owned())),
                                ..Default::default()
                            },
                        )
                        .await?;
                    (updated, Some(dir))
                }
                Ok(Err(e)) | Err(e) => {
                    tracing::warn!(path = %dest.display(), error = %e, "cover processing failed");
                    (edition, None)
                }
            }
        }
        (_, Some(existing)) => (edition.clone(), Some(PathBuf::from(existing))), // clone: edition returned in result
        (None, None) => (edition, None),
    };

    Ok(ImportResult {
        work,
        edition,
        file,
        authors,
        series,
        is_new_work,
        is_new_edition,
        cover_dir,
        warning: meta.warning.clone(), // clone: warning is reported in the result
    })
}

async fn find_or_create_work(
    db: &dyn Repository,
    title: &str,
    author_names: &[String],
    content_type: ContentType,
) -> Result<(Work, bool), ScannerError> {
    let candidates = db
        .list_works(
            &WorkFilter {
                title_contains: Some(title.to_string()),
                ..Default::default()
            },
            &Pagination {
                page: 1,
                per_page: MATCH_PAGE_SIZE,
            },
        )
        .await?
        .items
        .into_iter()
        .filter(|w| w.title.eq_ignore_ascii_case(title))
        .collect::<Vec<_>>();

    for candidate in candidates {
        if author_names.is_empty() {
            return Ok((candidate, false));
        }
        let linked = db.list_work_authors(candidate.id).await?;
        let shares_author = linked
            .iter()
            .any(|a| author_names.iter().any(|n| n.eq_ignore_ascii_case(&a.name)));
        if shares_author || linked.is_empty() {
            return Ok((candidate, false));
        }
    }

    let work = db
        .create_work(&NewWork {
            title: title.to_string(),
            sort_title: sort_title_for(title),
            original_language: None,
            original_pub_date: None,
            description: None,
            description_html: None,
            content_type,
            age_rating: None,
            content_warnings: vec![],
            read_status: ReadStatus::Unread,
            monitored: false,
        })
        .await?;
    Ok((work, true))
}

async fn find_or_create_author(db: &dyn Repository, name: &str) -> Result<Author, ScannerError> {
    let name = name.trim();
    let existing = db
        .list_authors(
            &AuthorFilter {
                name_contains: Some(name.to_string()),
                nationality: None,
            },
            &Pagination {
                page: 1,
                per_page: MATCH_PAGE_SIZE,
            },
        )
        .await?
        .items
        .into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(name));
    if let Some(author) = existing {
        return Ok(author);
    }
    Ok(db
        .create_author(&NewAuthor {
            name: name.to_string(),
            sort_name: sort_name_for(name),
            birth_date: None,
            death_date: None,
            nationality: None,
            biography: None,
            biography_html: None,
            external_ids: vec![],
        })
        .await?)
}

async fn find_or_create_publisher(
    db: &dyn Repository,
    name: &str,
) -> Result<curatarr_core::types::id::PublisherId, ScannerError> {
    let name = name.trim();
    let existing = db
        .list_publishers(
            &PublisherFilter {
                name_contains: Some(name.to_string()),
                country: None,
            },
            &Pagination {
                page: 1,
                per_page: MATCH_PAGE_SIZE,
            },
        )
        .await?
        .items
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name));
    if let Some(publisher) = existing {
        return Ok(publisher.id);
    }
    Ok(db
        .create_publisher(&NewPublisher {
            name: name.to_string(),
            sort_name: sort_title_for(name),
            imprint: None,
            parent_publisher_id: None,
            country: None,
            founding_year: None,
        })
        .await?
        .id)
}

async fn find_or_create_edition(
    db: &dyn Repository,
    work: &Work,
    meta: &ExtractedMetadata,
    format: FileFormat,
    publisher_id: Option<curatarr_core::types::id::PublisherId>,
) -> Result<(Edition, bool), ScannerError> {
    let existing = db
        .list_editions(
            &EditionFilter {
                work_id: Some(work.id),
                format: Some(format),
                language: None,
            },
            &Pagination {
                page: 1,
                per_page: 1,
            },
        )
        .await?
        .items
        .into_iter()
        .next();
    if let Some(edition) = existing {
        return Ok((edition, false));
    }

    let (isbn13, isbn10) = match meta.isbn.as_deref() {
        Some(raw) => (Isbn13::try_from(raw).ok(), Isbn10::try_from(raw).ok()),
        None => (None, None),
    };

    let edition = db
        .create_edition(&NewEdition {
            work_id: work.id,
            isbn13,
            isbn10,
            asin: None,
            publisher_id,
            imprint: None,
            publication_date: None,
            edition_number: None,
            format,
            page_count: meta.page_count,
            word_count: None,
            language: meta.language.clone(), // clone: new edition owns its values
            translator: None,
        })
        .await?;
    Ok((edition, true))
}

async fn attach_series(
    db: &dyn Repository,
    work_id: WorkId,
    series_title: &str,
    position: Option<f64>,
) -> Result<Series, ScannerError> {
    let series_title = series_title.trim();
    let existing = db
        .list_series(
            &SeriesFilter {
                title_contains: Some(series_title.to_string()),
                series_type: None,
            },
            &Pagination {
                page: 1,
                per_page: MATCH_PAGE_SIZE,
            },
        )
        .await?
        .items
        .into_iter()
        .find(|s| s.title.eq_ignore_ascii_case(series_title));

    let series = match existing {
        Some(series) => series,
        None => {
            db.create_series(&NewSeries {
                title: series_title.to_string(),
                sort_title: sort_title_for(series_title),
                description: None,
                series_type: SeriesType::Ongoing,
                reading_order: ReadingOrder::Publication,
                volume_count: None,
                expected_volume_count: None,
                external_ids: vec![],
                monitored: false,
            })
            .await?
        }
    };

    let already_linked = db
        .list_work_series_entries(work_id)
        .await?
        .iter()
        .any(|e| e.series_id == series.id);
    if !already_linked {
        db.create_series_entry(&NewSeriesEntry {
            series_id: series.id,
            work_id,
            position: position.unwrap_or(0.0),
            arc: None,
        })
        .await?;
    }
    Ok(series)
}

/// "The Left Hand of Darkness" → "Left Hand of Darkness, The"
pub fn sort_title_for(title: &str) -> String {
    let trimmed = title.trim();
    for article in ["The ", "A ", "An "] {
        if trimmed.len() > article.len() && trimmed[..article.len()].eq_ignore_ascii_case(article) {
            return format!(
                "{}, {}",
                &trimmed[article.len()..],
                trimmed[..article.len()].trim()
            );
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("The Left Hand of Darkness", "Left Hand of Darkness, The")]
    #[case("A Wizard of Earthsea", "Wizard of Earthsea, A")]
    #[case("An Unkindness of Ghosts", "Unkindness of Ghosts, An")]
    #[case("Dune", "Dune")]
    #[case("Theory of Everything", "Theory of Everything")]
    #[case("  The  ", "The")]
    fn sort_title_moves_leading_article(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(sort_title_for(input), expected);
    }

    #[test]
    fn unique_destination_appends_counter() {
        let dir = tempfile::tempdir().unwrap();
        let taken = dir.path().join("Dune.epub");
        std::fs::write(&taken, b"x").unwrap();
        let source = dir.path().join("elsewhere.epub");
        let unique = unique_destination(taken.clone(), &source);
        assert_eq!(unique, dir.path().join("Dune (2).epub"));

        std::fs::write(&unique, b"y").unwrap();
        let next = unique_destination(taken, &source);
        assert_eq!(next, dir.path().join("Dune (3).epub"));
    }

    #[test]
    fn unique_destination_keeps_source_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Dune.epub");
        std::fs::write(&path, b"x").unwrap();
        assert_eq!(unique_destination(path.clone(), &path), path);
    }
}
