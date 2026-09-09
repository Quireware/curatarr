use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::author::NewAuthor;
use curatarr_core::types::edition::NewEdition;
use curatarr_core::types::enums::*;
use curatarr_core::types::file::NewLibraryFile;
use curatarr_core::types::work::{NewWork, WorkFilter, WorkUpdate};
use curatarr_db::create_repository;
use std::sync::Arc;

async fn test_repo() -> Arc<dyn Repository> {
    create_repository("sqlite::memory:").await.unwrap()
}

fn new_work(title: &str, content_type: ContentType) -> NewWork {
    NewWork {
        title: title.into(),
        sort_title: title.into(),
        original_language: None,
        original_pub_date: None,
        description: None,
        description_html: None,
        content_type,
        age_rating: None,
        content_warnings: vec![],
        read_status: ReadStatus::Unread,
    }
}

#[tokio::test]
async fn work_crud_cycle() {
    let repo = test_repo().await;

    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();
    assert_eq!(work.title, "Dune");
    assert_eq!(work.content_type, ContentType::Book);

    let fetched = repo.get_work(work.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, work.id);
    assert_eq!(fetched.title, "Dune");

    let page = repo
        .list_works(&WorkFilter::default(), &Pagination::default())
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);

    let update = WorkUpdate {
        title: Some("Dune Messiah".into()),
        read_status: Some(ReadStatus::Reading),
        ..Default::default()
    };
    let updated = repo.update_work(work.id, &update).await.unwrap();
    assert_eq!(updated.title, "Dune Messiah");
    assert_eq!(updated.read_status, ReadStatus::Reading);

    repo.delete_work(work.id).await.unwrap();
    let gone = repo.get_work(work.id).await.unwrap();
    assert!(gone.is_none());
}

#[tokio::test]
async fn work_pagination() {
    let repo = test_repo().await;

    for i in 0..5 {
        repo.create_work(&new_work(&format!("Book {i}"), ContentType::Book))
            .await
            .unwrap();
    }

    let page1 = repo
        .list_works(
            &WorkFilter::default(),
            &Pagination {
                page: 1,
                per_page: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(page1.total, 5);
    assert_eq!(page1.items.len(), 2);

    let page3 = repo
        .list_works(
            &WorkFilter::default(),
            &Pagination {
                page: 3,
                per_page: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(page3.total, 5);
    assert_eq!(page3.items.len(), 1);
}

#[tokio::test]
async fn work_filter_by_content_type() {
    let repo = test_repo().await;

    repo.create_work(&new_work("Novel", ContentType::Book))
        .await
        .unwrap();
    repo.create_work(&new_work("One Piece", ContentType::Manga))
        .await
        .unwrap();
    repo.create_work(&new_work("Batman", ContentType::Comic))
        .await
        .unwrap();

    let filter = WorkFilter {
        content_type: Some(ContentType::Manga),
        ..Default::default()
    };
    let page = repo
        .list_works(&filter, &Pagination::default())
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].title, "One Piece");
}

#[tokio::test]
async fn cascade_delete_work_removes_editions() {
    let repo = test_repo().await;

    let work = repo
        .create_work(&new_work("Test", ContentType::Book))
        .await
        .unwrap();

    let edition = repo
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

    repo.delete_work(work.id).await.unwrap();

    let gone = repo.get_edition(edition.id).await.unwrap();
    assert!(gone.is_none());
}

#[tokio::test]
async fn find_file_by_hash() {
    let repo = test_repo().await;

    let work = repo
        .create_work(&new_work("Test", ContentType::Book))
        .await
        .unwrap();
    let edition = repo
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

    let file = repo
        .create_file(&NewLibraryFile {
            edition_id: edition.id,
            path: "/books/test.epub".into(),
            format: FileFormat::Epub,
            size_bytes: 12345,
            sha256: "abc123def456".into(),
        })
        .await
        .unwrap();

    let found = repo.find_file_by_hash("abc123def456").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, file.id);

    let not_found = repo.find_file_by_hash("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn author_crud_and_link() {
    let repo = test_repo().await;

    let author = repo
        .create_author(&NewAuthor {
            name: "Frank Herbert".into(),
            sort_name: "Herbert, Frank".into(),
            birth_date: None,
            death_date: None,
            nationality: Some("American".into()),
            biography: None,
            biography_html: None,
            external_ids: vec![],
        })
        .await
        .unwrap();

    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();

    repo.link_work_author(work.id, author.id, AuthorRole::Author)
        .await
        .unwrap();
    repo.unlink_work_author(work.id, author.id).await.unwrap();

    repo.delete_author(author.id).await.unwrap();
    assert!(repo.get_author(author.id).await.unwrap().is_none());
}

fn new_edition(work_id: curatarr_core::types::id::WorkId, format: FileFormat) -> NewEdition {
    NewEdition {
        work_id,
        isbn13: None,
        isbn10: None,
        asin: None,
        publisher_id: None,
        imprint: None,
        publication_date: None,
        edition_number: None,
        format,
        page_count: None,
        word_count: None,
        language: None,
        translator: None,
    }
}

#[tokio::test]
async fn update_work_sets_and_clears_nullable_fields() {
    use curatarr_core::types::work::WorkUpdate;
    let repo = test_repo().await;
    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();

    let updated = repo
        .update_work(
            work.id,
            &WorkUpdate {
                description: Some(Some("Desert planet".into())),
                average_rating: Some(Some(4.5)),
                age_rating: Some(Some(AgeRating::Teen)),
                user_notes: Some(Some("notes".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.description.as_deref(), Some("Desert planet"));
    assert_eq!(updated.average_rating, Some(4.5));
    assert_eq!(updated.age_rating, Some(AgeRating::Teen));

    let cleared = repo
        .update_work(
            work.id,
            &WorkUpdate {
                description: Some(None),
                average_rating: Some(None),
                age_rating: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cleared.description, None);
    assert_eq!(cleared.average_rating, None);
    assert_eq!(cleared.age_rating, None);
    assert_eq!(cleared.user_notes.as_deref(), Some("notes"));
}

#[tokio::test]
async fn update_edition_sets_cover_and_page_count() {
    use curatarr_core::types::edition::EditionUpdate;
    let repo = test_repo().await;
    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();
    let edition = repo
        .create_edition(&new_edition(work.id, FileFormat::Epub))
        .await
        .unwrap();

    let updated = repo
        .update_edition(
            edition.id,
            &EditionUpdate {
                cover_path: Some(Some("/covers/abc".into())),
                page_count: Some(Some(412)),
                language: Some(Some("en".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.cover_path.as_deref(), Some("/covers/abc"));
    assert_eq!(updated.page_count, Some(412));
    assert_eq!(updated.language.as_deref(), Some("en"));
}

#[tokio::test]
async fn update_file_deleted_at_roundtrip_and_find_by_path() {
    use curatarr_core::types::file::LibraryFileUpdate;
    let repo = test_repo().await;
    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();
    let edition = repo
        .create_edition(&new_edition(work.id, FileFormat::Epub))
        .await
        .unwrap();
    let file = repo
        .create_file(&NewLibraryFile {
            edition_id: edition.id,
            path: "/books/dune.epub".into(),
            format: FileFormat::Epub,
            size_bytes: 10,
            sha256: "h1".into(),
        })
        .await
        .unwrap();

    assert!(
        repo.find_file_by_path("/books/dune.epub")
            .await
            .unwrap()
            .is_some()
    );
    assert!(repo.find_file_by_path("/nope").await.unwrap().is_none());

    let now = chrono::Utc::now();
    let deleted = repo
        .update_file(
            file.id,
            &LibraryFileUpdate {
                path: None,
                deleted_at: Some(Some(now)),
            },
        )
        .await
        .unwrap();
    assert!(deleted.deleted_at.is_some());
    // soft-deleted files are excluded from hash lookup and default listing
    assert!(repo.find_file_by_hash("h1").await.unwrap().is_none());
    let visible = repo
        .list_files(
            &curatarr_core::types::file::FileFilter::default(),
            &Pagination::default(),
        )
        .await
        .unwrap();
    assert_eq!(visible.total, 0);

    let restored = repo
        .update_file(
            file.id,
            &LibraryFileUpdate {
                path: Some("/books/moved.epub".into()),
                deleted_at: Some(None),
            },
        )
        .await
        .unwrap();
    assert!(restored.deleted_at.is_none());
    assert_eq!(restored.path, "/books/moved.epub");
}

#[tokio::test]
async fn list_work_authors_returns_linked_authors() {
    let repo = test_repo().await;
    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();
    let author = repo
        .create_author(&NewAuthor {
            name: "Frank Herbert".into(),
            sort_name: "Herbert, Frank".into(),
            birth_date: None,
            death_date: None,
            nationality: None,
            biography: None,
            biography_html: None,
            external_ids: vec![],
        })
        .await
        .unwrap();
    assert!(repo.list_work_authors(work.id).await.unwrap().is_empty());
    repo.link_work_author(work.id, author.id, AuthorRole::Author)
        .await
        .unwrap();
    let authors = repo.list_work_authors(work.id).await.unwrap();
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0].id, author.id);
}

#[tokio::test]
async fn root_folder_crud_and_unique_path() {
    use curatarr_core::types::root_folder::NewRootFolder;
    let repo = test_repo().await;

    let folder = repo
        .create_root_folder(&NewRootFolder {
            path: "/books".into(),
            name: Some("Main".into()),
            content_types: vec![ContentType::Book, ContentType::Manga],
        })
        .await
        .unwrap();
    assert_eq!(folder.path, "/books");
    assert_eq!(
        folder.content_types,
        vec![ContentType::Book, ContentType::Manga]
    );

    let fetched = repo.get_root_folder(folder.id).await.unwrap().unwrap();
    assert_eq!(fetched, folder);
    assert_eq!(repo.list_root_folders().await.unwrap().len(), 1);

    let dup = repo
        .create_root_folder(&NewRootFolder {
            path: "/books".into(),
            name: None,
            content_types: vec![],
        })
        .await;
    assert!(matches!(
        dup,
        Err(curatarr_core::error::DbError::Conflict(_))
    ));

    repo.delete_root_folder(folder.id).await.unwrap();
    assert!(repo.get_root_folder(folder.id).await.unwrap().is_none());
}

#[tokio::test]
async fn recycle_entry_crud() {
    use curatarr_core::types::id::FileId;
    use curatarr_core::types::recycle::NewRecycleEntry;
    let repo = test_repo().await;
    let file_id = FileId::new();

    let entry = repo
        .create_recycle_entry(&NewRecycleEntry {
            original_file_id: file_id,
            original_path: "/books/a.epub".into(),
            recycle_path: "/data/recycle/a.epub".into(),
        })
        .await
        .unwrap();
    assert_eq!(entry.original_file_id, file_id);

    let found = repo.get_recycle_entry_for_file(file_id).await.unwrap();
    assert_eq!(found, Some(entry.clone()));
    assert_eq!(repo.list_recycle_entries().await.unwrap().len(), 1);

    repo.delete_recycle_entry(entry.id).await.unwrap();
    assert!(
        repo.get_recycle_entry_for_file(file_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn migrations_are_idempotent_on_persistent_database() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("curatarr.db").display()
    );

    let first = create_repository(&url).await.unwrap();
    let work = first
        .create_work(&new_work("Persisted", ContentType::Book))
        .await
        .unwrap();
    drop(first);

    // Second open of the same file must not re-run CREATE TABLE and must keep the data.
    let second = create_repository(&url).await.unwrap();
    let fetched = second.get_work(work.id).await.unwrap();
    assert_eq!(fetched.map(|w| w.title).as_deref(), Some("Persisted"));

    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    let applied = curatarr_db::run_migrations(&pool).await.unwrap();
    assert!(
        applied.is_empty(),
        "no migrations should apply on a third run"
    );
}

#[tokio::test]
async fn metadata_locks_sources_audit_and_lookups() {
    use curatarr_core::types::identifiers::ExternalId;
    use curatarr_core::types::metadata::{EntityKind, NewAuditEvent, fields};

    let repo = test_repo().await;
    let work = repo
        .create_work(&new_work("Dune", ContentType::Book))
        .await
        .unwrap();
    let id = work.id.to_string();

    repo.set_field_lock(EntityKind::Work, &id, fields::TITLE)
        .await
        .unwrap();
    let locks = repo.list_field_locks(EntityKind::Work, &id).await.unwrap();
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].field, fields::TITLE);

    repo.upsert_field_source(EntityKind::Work, &id, fields::TITLE, "openlibrary")
        .await
        .unwrap();
    let sources = repo
        .list_field_sources(EntityKind::Work, &id)
        .await
        .unwrap();
    assert_eq!(sources[0].source, "openlibrary");

    let ext = ExternalId::OpenLibraryWork("OL45883W".into());
    repo.upsert_external_id(EntityKind::Work, &id, &ext)
        .await
        .unwrap();
    let ids = repo.list_external_ids(EntityKind::Work, &id).await.unwrap();
    assert_eq!(ids, vec![ext]);

    repo.append_audit(&NewAuditEvent {
        entity_kind: EntityKind::Work,
        entity_id: id.clone(),
        field: fields::TITLE.into(),
        old_value: Some("Dune".into()),
        new_value: Some("Dune Messiah".into()),
        source: "openlibrary".into(),
    })
    .await
    .unwrap();
    let page = repo
        .list_audit(EntityKind::Work, &id, &Pagination::default())
        .await
        .unwrap();
    assert_eq!(page.total, 1);

    repo.create_author(&NewAuthor {
        name: "Frank Herbert".into(),
        sort_name: "Herbert, Frank".into(),
        birth_date: None,
        death_date: None,
        nationality: None,
        biography: None,
        biography_html: None,
        external_ids: vec![],
    })
    .await
    .unwrap();
    let found = repo
        .find_author_by_name("frank herbert")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.name, "Frank Herbert");
}
