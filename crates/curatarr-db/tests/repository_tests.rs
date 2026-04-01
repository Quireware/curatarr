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
