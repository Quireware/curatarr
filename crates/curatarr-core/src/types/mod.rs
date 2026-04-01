pub mod author;
pub mod collection;
pub mod edition;
pub mod enums;
pub mod file;
pub mod id;
pub mod identifiers;
pub mod publisher;
pub mod series;
pub mod tag;
pub mod work;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
}

impl<T> Page<T> {
    pub fn empty(pagination: &Pagination) -> Self {
        Self {
            items: Vec::new(),
            total: 0,
            page: pagination.page,
            per_page: pagination.per_page,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn pagination_serde_roundtrip(page in 1u32..1000, per_page in 1u32..100) {
            let p = Pagination { page, per_page };
            let json = serde_json::to_string(&p).unwrap();
            let back: Pagination = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(p, back);
        }
    }

    #[test]
    fn page_empty_has_correct_metadata() {
        let pagination = Pagination {
            page: 3,
            per_page: 10,
        };
        let page: Page<String> = Page::empty(&pagination);
        assert!(page.items.is_empty());
        assert_eq!(page.total, 0);
        assert_eq!(page.page, 3);
        assert_eq!(page.per_page, 10);
    }

    #[test]
    fn work_serde_roundtrip() {
        use super::enums::{ContentType, ReadStatus};
        use super::id::WorkId;
        use super::work::Work;

        let now = Utc::now();
        let work = Work {
            id: WorkId::new(),
            title: "Dune".into(),
            sort_title: "Dune".into(),
            original_language: Some("en".into()),
            original_pub_date: None,
            description: Some("A desert planet saga".into()),
            description_html: None,
            content_type: ContentType::Book,
            age_rating: None,
            content_warnings: vec!["violence".into()],
            average_rating: Some(4.5),
            user_rating: None,
            user_review: None,
            read_status: ReadStatus::WantToRead,
            user_notes: None,
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_string(&work).unwrap();
        let back: Work = serde_json::from_str(&json).unwrap();
        assert_eq!(work, back);
    }

    #[test]
    fn edition_serde_roundtrip() {
        use super::edition::Edition;
        use super::enums::FileFormat;
        use super::id::{EditionId, WorkId};
        use super::identifiers::Isbn13;

        let now = Utc::now();
        let edition = Edition {
            id: EditionId::new(),
            work_id: WorkId::new(),
            isbn13: Some(Isbn13::try_from("9780306406157").unwrap()),
            isbn10: None,
            asin: None,
            publisher_id: None,
            imprint: None,
            publication_date: None,
            edition_number: Some(1),
            format: FileFormat::Epub,
            page_count: Some(412),
            word_count: Some(188_000),
            language: Some("en".into()),
            translator: None,
            cover_path: None,
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_string(&edition).unwrap();
        let back: Edition = serde_json::from_str(&json).unwrap();
        assert_eq!(edition, back);
    }

    #[test]
    fn author_serde_roundtrip() {
        use super::author::Author;
        use super::id::AuthorId;
        use super::identifiers::ExternalId;

        let now = Utc::now();
        let author = Author {
            id: AuthorId::new(),
            name: "Frank Herbert".into(),
            sort_name: "Herbert, Frank".into(),
            birth_date: None,
            death_date: None,
            nationality: Some("American".into()),
            biography: None,
            biography_html: None,
            photo_path: None,
            external_ids: vec![ExternalId::OpenLibraryWork("OL34675A".into())],
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_string(&author).unwrap();
        let back: Author = serde_json::from_str(&json).unwrap();
        assert_eq!(author, back);
    }

    #[test]
    fn series_serde_roundtrip() {
        use super::enums::{ReadingOrder, SeriesType};
        use super::id::SeriesId;
        use super::series::Series;

        let now = Utc::now();
        let series = Series {
            id: SeriesId::new(),
            title: "Dune".into(),
            sort_title: "Dune".into(),
            description: None,
            series_type: SeriesType::Completed,
            reading_order: ReadingOrder::Publication,
            volume_count: Some(6),
            expected_volume_count: Some(6),
            external_ids: vec![],
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_string(&series).unwrap();
        let back: Series = serde_json::from_str(&json).unwrap();
        assert_eq!(series, back);
    }

    #[test]
    fn library_file_serde_roundtrip() {
        use super::enums::FileFormat;
        use super::file::LibraryFile;
        use super::id::{EditionId, FileId};

        let now = Utc::now();
        let file = LibraryFile {
            id: FileId::new(),
            edition_id: EditionId::new(),
            path: "/books/Dune/Dune.epub".into(),
            format: FileFormat::Epub,
            size_bytes: 1_234_567,
            sha256: "abc123def456".into(),
            import_date: now,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_string(&file).unwrap();
        let back: LibraryFile = serde_json::from_str(&json).unwrap();
        assert_eq!(file, back);
    }
}
