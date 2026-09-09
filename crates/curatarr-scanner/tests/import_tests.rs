use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::enums::{ContentType, FileFormat, ImportMode};
use curatarr_core::types::file::FileFilter;
use curatarr_db::create_repository;
use curatarr_scanner::import::{
    ImportConfig, ImportEvent, ImportOutcome, import_directory, import_directory_with_progress,
    import_file,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Fixture {
    _dir: tempfile::TempDir,
    inbox: PathBuf,
    library: PathBuf,
    covers: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("inbox");
    let library = dir.path().join("library");
    let covers = dir.path().join("covers");
    std::fs::create_dir_all(&inbox).unwrap();
    Fixture {
        _dir: dir,
        inbox,
        library,
        covers,
    }
}

fn config(f: &Fixture, mode: ImportMode, organise: bool) -> ImportConfig {
    ImportConfig {
        mode,
        library_root: f.library.clone(),
        naming_template: "{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}".into(),
        exclusions: vec!["*.tmp".into()],
        covers_dir: f.covers.clone(),
        organise,
    }
}

async fn repo() -> Arc<dyn Repository> {
    create_repository("sqlite::memory:").await.unwrap()
}

fn png_bytes() -> Vec<u8> {
    let img = image::DynamicImage::new_rgb8(40, 60);
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

struct EpubSpec<'a> {
    title: &'a str,
    author: &'a str,
    series: Option<(&'a str, f64)>,
    with_cover: bool,
    /// makes the bytes differ so hashes differ
    salt: &'a str,
}

fn write_epub(path: &Path, spec: &EpubSpec<'_>) {
    let series_meta = spec
        .series
        .map(|(name, idx)| {
            format!(
                r#"<meta name="calibre:series" content="{name}"/><meta name="calibre:series_index" content="{idx}"/>"#
            )
        })
        .unwrap_or_default();
    let cover_meta = if spec.with_cover {
        r#"<meta name="cover" content="cover-image"/>"#
    } else {
        ""
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:language>en</dc:language>
    <dc:date>1965-08-01</dc:date>
    <dc:publisher>Chilton</dc:publisher>
    {series_meta}{cover_meta}
  </metadata>
  <manifest>
    <item id="cover-image" href="images/cover.png" media-type="image/png"/>
  </manifest>
  <spine/>
</package>"#,
        title = spec.title,
        author = spec.author,
    );
    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", opts).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", opts).unwrap();
    zip.write_all(container.as_bytes()).unwrap();
    zip.start_file("OEBPS/content.opf", opts).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    if spec.with_cover {
        zip.start_file("OEBPS/images/cover.png", opts).unwrap();
        zip.write_all(&png_bytes()).unwrap();
    }
    zip.start_file("salt.txt", opts).unwrap();
    zip.write_all(spec.salt.as_bytes()).unwrap();
    zip.finish().unwrap();
}

fn dune(with_cover: bool) -> EpubSpec<'static> {
    EpubSpec {
        title: "Dune",
        author: "Frank Herbert",
        series: Some(("Dune Chronicles", 1.0)),
        with_cover,
        salt: "dune",
    }
}

#[tokio::test]
async fn import_new_epub_creates_all_records_and_cover() {
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("dune.epub");
    write_epub(&src, &dune(true));

    let outcome = import_file(&src, &config(&f, ImportMode::Copy, true), db.as_ref())
        .await
        .unwrap();
    let ImportOutcome::Imported(result) = outcome else {
        panic!("expected import");
    };

    assert_eq!(result.work.title, "Dune");
    assert_eq!(result.work.content_type, ContentType::Book);
    assert!(result.is_new_work);
    assert!(result.is_new_edition);
    assert_eq!(result.edition.format, FileFormat::Epub);
    assert_eq!(result.edition.language.as_deref(), Some("en"));
    assert!(result.edition.publisher_id.is_some());
    assert_eq!(result.authors.len(), 1);
    assert_eq!(result.authors[0].name, "Frank Herbert");
    assert_eq!(result.authors[0].sort_name, "Herbert, Frank");
    assert_eq!(
        result.series.as_ref().map(|s| s.title.as_str()),
        Some("Dune Chronicles")
    );

    let expected = f
        .library
        .join("Frank Herbert/Dune Chronicles/01 - Dune.epub");
    assert_eq!(PathBuf::from(&result.file.path), expected);
    assert!(expected.exists());

    let cover_dir = result.cover_dir.expect("cover processed");
    assert!(cover_dir.join("thumb_64.jpg").exists());
    assert_eq!(
        result.edition.cover_path.as_deref(),
        Some(cover_dir.to_string_lossy().as_ref())
    );

    let entries = db.list_work_series_entries(result.work.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].position, 1.0);
    assert_eq!(db.list_work_authors(result.work.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn importing_same_content_twice_is_duplicate() {
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("dune.epub");
    write_epub(&src, &dune(false));
    let cfg = config(&f, ImportMode::Copy, true);

    let first = import_file(&src, &cfg, db.as_ref()).await.unwrap();
    assert!(matches!(first, ImportOutcome::Imported(_)));

    let again = import_file(&src, &cfg, db.as_ref()).await.unwrap();
    let ImportOutcome::Duplicate { existing, .. } = again else {
        panic!("expected duplicate");
    };
    assert!(existing.path.ends_with("01 - Dune.epub"));

    let files = db
        .list_files(&FileFilter::default(), &Pagination::default())
        .await
        .unwrap();
    assert_eq!(files.total, 1);
}

#[tokio::test]
async fn copy_mode_keeps_original() {
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("dune.epub");
    write_epub(&src, &dune(false));
    import_file(&src, &config(&f, ImportMode::Copy, true), db.as_ref())
        .await
        .unwrap();
    assert!(src.exists());
}

#[tokio::test]
async fn move_mode_removes_original() {
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("dune.epub");
    write_epub(&src, &dune(false));
    let outcome = import_file(&src, &config(&f, ImportMode::Move, true), db.as_ref())
        .await
        .unwrap();
    let ImportOutcome::Imported(result) = outcome else {
        panic!("expected import");
    };
    assert!(!src.exists());
    assert!(Path::new(&result.file.path).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn hardlink_mode_shares_inode() {
    use std::os::unix::fs::MetadataExt;
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("dune.epub");
    write_epub(&src, &dune(false));
    let outcome = import_file(&src, &config(&f, ImportMode::Hardlink, true), db.as_ref())
        .await
        .unwrap();
    let ImportOutcome::Imported(result) = outcome else {
        panic!("expected import");
    };
    let a = std::fs::metadata(&src).unwrap();
    let b = std::fs::metadata(&result.file.path).unwrap();
    assert_eq!(a.ino(), b.ino());
    assert_eq!(a.nlink(), 2);
}

#[tokio::test]
async fn in_place_scan_leaves_file_where_it_is() {
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("odd name/dune.epub");
    write_epub(&src, &dune(false));
    let outcome = import_file(&src, &config(&f, ImportMode::Move, false), db.as_ref())
        .await
        .unwrap();
    let ImportOutcome::Imported(result) = outcome else {
        panic!("expected import");
    };
    assert_eq!(PathBuf::from(&result.file.path), src);
    assert!(src.exists());
    assert!(!f.library.exists());
}

#[tokio::test]
async fn second_format_of_same_work_reuses_work_and_author() {
    let f = fixture();
    let db = repo().await;
    let epub = f.inbox.join("dune.epub");
    write_epub(&epub, &dune(false));
    let cfg = config(&f, ImportMode::Copy, true);
    let first = import_file(&epub, &cfg, db.as_ref()).await.unwrap();
    let ImportOutcome::Imported(first) = first else {
        panic!()
    };

    // Same title/author but different bytes and a different format (a PDF stub via extension).
    let pdf = f.inbox.join("Dune.pdf");
    std::fs::write(&pdf, b"%PDF-1.4\n%not really\n").unwrap();
    let second = import_file(&pdf, &cfg, db.as_ref()).await.unwrap();
    let ImportOutcome::Imported(second) = second else {
        panic!()
    };
    // A bare PDF has no author metadata, so it matches the existing "Dune" work by title.
    assert_eq!(second.work.id, first.work.id);
    assert!(!second.is_new_work);
    assert!(second.is_new_edition);
    assert_eq!(second.edition.format, FileFormat::Pdf);
}

#[tokio::test]
async fn import_directory_reports_each_outcome() {
    let f = fixture();
    let db = repo().await;
    write_epub(&f.inbox.join("a/dune.epub"), &dune(false));
    write_epub(
        &f.inbox.join("b/messiah.epub"),
        &EpubSpec {
            title: "Dune Messiah",
            author: "Frank Herbert",
            series: Some(("Dune Chronicles", 2.0)),
            with_cover: false,
            salt: "messiah",
        },
    );
    // identical bytes to a/dune.epub → duplicate
    std::fs::create_dir_all(f.inbox.join("c")).unwrap();
    std::fs::copy(
        f.inbox.join("a/dune.epub"),
        f.inbox.join("c/dune copy.epub"),
    )
    .unwrap();
    // excluded by pattern
    std::fs::write(f.inbox.join("junk.epub.tmp"), b"x").unwrap();
    // unsupported, skipped by scanner
    std::fs::write(f.inbox.join("notes.txt"), b"x").unwrap();

    let mut events = Vec::new();
    let report = import_directory_with_progress(
        &f.inbox,
        &config(&f, ImportMode::Copy, true),
        db.as_ref(),
        |e| events.push(e.clone()),
    )
    .await
    .unwrap();

    assert_eq!(report.imported.len(), 2);
    assert_eq!(report.duplicates.len(), 1);
    assert!(report.failed.is_empty());
    assert_eq!(report.total(), 3);
    assert!(matches!(events[0], ImportEvent::Discovered { total: 3 }));

    // both volumes share one series and one author
    assert_eq!(
        report.imported[0].authors[0].id,
        report.imported[1].authors[0].id
    );
    let series_id = report.imported[0].series.as_ref().unwrap().id;
    assert_eq!(report.imported[1].series.as_ref().unwrap().id, series_id);
    assert_eq!(db.list_series_entries(series_id).await.unwrap().len(), 2);

    assert!(
        f.library
            .join("Frank Herbert/Dune Chronicles/02 - Dune Messiah.epub")
            .exists()
    );
}

#[tokio::test]
async fn corrupt_file_still_imports_with_warning() {
    let f = fixture();
    let db = repo().await;
    let src = f.inbox.join("Broken Book.epub");
    std::fs::write(&src, b"this is not a zip archive at all").unwrap();

    let outcome = import_file(&src, &config(&f, ImportMode::Copy, true), db.as_ref())
        .await
        .unwrap();
    let ImportOutcome::Imported(result) = outcome else {
        panic!("expected import");
    };
    assert!(result.warning.is_some());
    assert_eq!(result.work.title, "Broken Book");
    assert!(result.authors.is_empty());
    assert!(f.library.join("Unknown Author/Broken Book.epub").exists());
}

#[tokio::test]
async fn import_directory_on_missing_root_errors() {
    let f = fixture();
    let db = repo().await;
    let result = import_directory(
        &f.inbox.join("missing"),
        &config(&f, ImportMode::Copy, true),
        db.as_ref(),
    )
    .await;
    assert!(result.is_err());
}
