use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Book,
    Comic,
    Manga,
    GraphicNovel,
    LightNovel,
    Webtoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadStatus {
    Unread,
    Reading,
    Read,
    Abandoned,
    WantToRead,
}

impl Default for ReadStatus {
    fn default() -> Self {
        Self::Unread
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeRating {
    Children,
    Teen,
    Mature,
    Adult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    Epub,
    Mobi,
    Azw3,
    Cbz,
    Cbr,
    Cb7,
    Cbt,
    Pdf,
    Djvu,
    Fb2,
    WebpFolder,
}

impl FileFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Epub => "epub",
            Self::Mobi => "mobi",
            Self::Azw3 => "azw3",
            Self::Cbz => "cbz",
            Self::Cbr => "cbr",
            Self::Cb7 => "cb7",
            Self::Cbt => "cbt",
            Self::Pdf => "pdf",
            Self::Djvu => "djvu",
            Self::Fb2 => "fb2",
            Self::WebpFolder => "webp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesType {
    Ongoing,
    Completed,
    Hiatus,
    Cancelled,
    OneShot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingOrder {
    Publication,
    Chronological,
    Recommended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    Move,
    Copy,
    Hardlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorRole {
    Author,
    CoAuthor,
    Illustrator,
    Colourist,
    Letterer,
    Translator,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Searching,
    Grabbed,
    Downloading,
    Importing,
    Imported,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(ContentType::Book, "\"book\"")]
    #[case(ContentType::Comic, "\"comic\"")]
    #[case(ContentType::Manga, "\"manga\"")]
    #[case(ContentType::GraphicNovel, "\"graphic_novel\"")]
    #[case(ContentType::LightNovel, "\"light_novel\"")]
    #[case(ContentType::Webtoon, "\"webtoon\"")]
    fn content_type_serializes(#[case] variant: ContentType, #[case] expected: &str) {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }

    #[rstest]
    #[case(ReadStatus::Unread, "\"unread\"")]
    #[case(ReadStatus::Reading, "\"reading\"")]
    #[case(ReadStatus::Read, "\"read\"")]
    #[case(ReadStatus::Abandoned, "\"abandoned\"")]
    #[case(ReadStatus::WantToRead, "\"want_to_read\"")]
    fn read_status_serializes(#[case] variant: ReadStatus, #[case] expected: &str) {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }

    #[rstest]
    #[case(FileFormat::Epub, "\"epub\"")]
    #[case(FileFormat::Mobi, "\"mobi\"")]
    #[case(FileFormat::Azw3, "\"azw3\"")]
    #[case(FileFormat::Cbz, "\"cbz\"")]
    #[case(FileFormat::Cbr, "\"cbr\"")]
    #[case(FileFormat::Cb7, "\"cb7\"")]
    #[case(FileFormat::Cbt, "\"cbt\"")]
    #[case(FileFormat::Pdf, "\"pdf\"")]
    #[case(FileFormat::Djvu, "\"djvu\"")]
    #[case(FileFormat::Fb2, "\"fb2\"")]
    #[case(FileFormat::WebpFolder, "\"webp_folder\"")]
    fn file_format_serializes(#[case] variant: FileFormat, #[case] expected: &str) {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }

    #[rstest]
    #[case(SeriesType::Ongoing, "\"ongoing\"")]
    #[case(SeriesType::Completed, "\"completed\"")]
    #[case(SeriesType::Hiatus, "\"hiatus\"")]
    #[case(SeriesType::Cancelled, "\"cancelled\"")]
    #[case(SeriesType::OneShot, "\"one_shot\"")]
    fn series_type_serializes(#[case] variant: SeriesType, #[case] expected: &str) {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }

    #[rstest]
    #[case(AuthorRole::Author, "\"author\"")]
    #[case(AuthorRole::CoAuthor, "\"co_author\"")]
    #[case(AuthorRole::Illustrator, "\"illustrator\"")]
    #[case(AuthorRole::Colourist, "\"colourist\"")]
    #[case(AuthorRole::Letterer, "\"letterer\"")]
    #[case(AuthorRole::Translator, "\"translator\"")]
    #[case(AuthorRole::Editor, "\"editor\"")]
    fn author_role_serializes(#[case] variant: AuthorRole, #[case] expected: &str) {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }

    #[rstest]
    #[case(DownloadState::Searching, "\"searching\"")]
    #[case(DownloadState::Grabbed, "\"grabbed\"")]
    #[case(DownloadState::Downloading, "\"downloading\"")]
    #[case(DownloadState::Importing, "\"importing\"")]
    #[case(DownloadState::Imported, "\"imported\"")]
    #[case(DownloadState::Failed, "\"failed\"")]
    fn download_state_serializes(#[case] variant: DownloadState, #[case] expected: &str) {
        assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
    }

    macro_rules! roundtrip_test {
        ($name:ident, $type:ty, $( $variant:expr ),+ $(,)?) => {
            #[test]
            fn $name() {
                for variant in &[$( $variant ),+] {
                    let json = serde_json::to_string(variant).unwrap();
                    let back: $type = serde_json::from_str(&json).unwrap();
                    assert_eq!(*variant, back, "Roundtrip failed for {json}");
                }
            }
        };
    }

    roundtrip_test!(
        content_type_roundtrip,
        ContentType,
        ContentType::Book,
        ContentType::Comic,
        ContentType::Manga,
        ContentType::GraphicNovel,
        ContentType::LightNovel,
        ContentType::Webtoon
    );

    roundtrip_test!(
        age_rating_roundtrip,
        AgeRating,
        AgeRating::Children,
        AgeRating::Teen,
        AgeRating::Mature,
        AgeRating::Adult
    );

    roundtrip_test!(
        import_mode_roundtrip,
        ImportMode,
        ImportMode::Move,
        ImportMode::Copy,
        ImportMode::Hardlink
    );

    roundtrip_test!(
        reading_order_roundtrip,
        ReadingOrder,
        ReadingOrder::Publication,
        ReadingOrder::Chronological,
        ReadingOrder::Recommended
    );
}
