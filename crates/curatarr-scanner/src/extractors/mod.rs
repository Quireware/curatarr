pub mod comic;
pub mod epub;
pub mod pdf;

use curatarr_core::types::enums::{ContentType, FileFormat};
use std::path::Path;

/// Format-independent metadata gathered from a file, ready for the import pipeline.
///
/// Extraction never fails outright: when a format-specific extractor errors, the
/// result degrades to filename-derived metadata and the error is recorded in `warning`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    /// Primary creators (writers). Linked with `AuthorRole::Author`.
    pub authors: Vec<String>,
    /// Visual artists for comics. Linked with `AuthorRole::Illustrator`.
    pub illustrators: Vec<String>,
    pub series: Option<String>,
    pub series_position: Option<f64>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub year: Option<i32>,
    pub page_count: Option<u32>,
    pub cover: Option<Vec<u8>>,
    pub content_type: Option<ContentType>,
    /// Extraction problem, if any; the remaining fields are then filename-derived.
    pub warning: Option<String>,
}

impl ExtractedMetadata {
    /// Title, or the file stem when the file carried no title.
    pub fn title_or_stem(&self, path: &Path) -> String {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| stem_of(path))
    }
}

pub fn extract_metadata(path: &Path, format: FileFormat) -> ExtractedMetadata {
    let mut meta = match format {
        FileFormat::Epub => epub::extract_epub_metadata(path).map(|m| {
            let cover = epub::extract_epub_cover(path, &m);
            ExtractedMetadata {
                title: m.title,
                authors: m.authors,
                illustrators: vec![],
                series: m.series,
                series_position: m.series_position,
                language: m.language,
                description: m.description,
                isbn: m.isbn,
                publisher: m.publisher,
                year: m.pub_date.as_deref().and_then(leading_year),
                page_count: None,
                cover,
                content_type: Some(ContentType::Book),
                warning: None,
            }
        }),
        FileFormat::Cbz | FileFormat::Cbr | FileFormat::Cb7 | FileFormat::Cbt => {
            comic::extract_comic_metadata(path, format).map(|m| {
                let title = m.title.or_else(|| {
                    m.series.as_ref().map(|s| match &m.issue {
                        Some(issue) => format!("{s} #{issue}"),
                        None => s.to_string(),
                    })
                });
                ExtractedMetadata {
                    title,
                    authors: m.writer.into_iter().collect(),
                    illustrators: m.artist.into_iter().collect(),
                    series: m.series,
                    series_position: m.issue.as_deref().and_then(|i| i.trim().parse().ok()),
                    language: None,
                    description: None,
                    isbn: None,
                    publisher: None,
                    year: m.year.as_deref().and_then(leading_year),
                    page_count: m.page_count,
                    cover: m.cover_data,
                    content_type: Some(if m.manga {
                        ContentType::Manga
                    } else {
                        ContentType::Comic
                    }),
                    warning: None,
                }
            })
        }
        FileFormat::Pdf => pdf::extract_pdf_metadata(path).map(|m| ExtractedMetadata {
            title: m.title,
            authors: m.author.into_iter().collect(),
            illustrators: vec![],
            series: None,
            series_position: None,
            language: None,
            description: None,
            isbn: None,
            publisher: None,
            year: m.creation_date.as_deref().and_then(pdf_year),
            page_count: m.page_count,
            cover: None,
            content_type: Some(ContentType::Book),
            warning: None,
        }),
        FileFormat::Mobi
        | FileFormat::Azw3
        | FileFormat::Djvu
        | FileFormat::Fb2
        | FileFormat::WebpFolder => Ok(ExtractedMetadata {
            content_type: Some(ContentType::Book),
            ..Default::default()
        }),
    }
    .unwrap_or_else(|e| ExtractedMetadata {
        warning: Some(e.to_string()),
        content_type: Some(default_content_type(format)),
        ..Default::default()
    });

    if meta.title.as_deref().is_none_or(|t| t.trim().is_empty()) {
        meta.title = Some(stem_of(path));
    }
    if meta.content_type.is_none() {
        meta.content_type = Some(default_content_type(format));
    }
    meta
}

pub fn default_content_type(format: FileFormat) -> ContentType {
    match format {
        FileFormat::Cbz | FileFormat::Cbr | FileFormat::Cb7 | FileFormat::Cbt => ContentType::Comic,
        _ => ContentType::Book,
    }
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Unknown Title".to_string())
}

/// First run of four digits in a date-ish string: "2019-03-01" → 2019, "1965" → 1965.
fn leading_year(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    for start in 0..bytes.len() {
        if bytes.len() - start < 4 {
            break;
        }
        let run = &s[start..start + 4];
        if run.bytes().all(|b| b.is_ascii_digit()) {
            return run.parse().ok();
        }
    }
    None
}

/// PDF dates look like `D:20190301120000Z`.
fn pdf_year(s: &str) -> Option<i32> {
    leading_year(s.trim_start_matches("D:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("2019-03-01", Some(2019))]
    #[case("1965", Some(1965))]
    #[case("March 1965", Some(1965))]
    #[case("n/a", None)]
    #[case("", None)]
    #[case("12", None)]
    fn leading_year_cases(#[case] input: &str, #[case] expected: Option<i32>) {
        assert_eq!(leading_year(input), expected);
    }

    #[test]
    fn pdf_year_strips_prefix() {
        assert_eq!(pdf_year("D:20190301120000Z"), Some(2019));
    }

    #[test]
    fn unsupported_extractor_falls_back_to_stem() {
        let f = tempfile::Builder::new().suffix(".mobi").tempfile().unwrap();
        let meta = extract_metadata(f.path(), FileFormat::Mobi);
        assert!(meta.title.is_some());
        assert_eq!(meta.content_type, Some(ContentType::Book));
    }

    #[test]
    fn broken_epub_degrades_with_warning() {
        let f = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
        std::fs::write(f.path(), b"not a zip").unwrap();
        let meta = extract_metadata(f.path(), FileFormat::Epub);
        assert!(meta.warning.is_some());
        assert!(meta.title.is_some());
        assert_eq!(meta.content_type, Some(ContentType::Book));
    }

    #[test]
    fn title_or_stem_uses_stem_when_blank() {
        let meta = ExtractedMetadata {
            title: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(meta.title_or_stem(Path::new("/x/Dune.epub")), "Dune");
    }
}
