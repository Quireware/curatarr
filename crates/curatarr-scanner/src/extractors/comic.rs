use curatarr_core::error::ScannerError;
use curatarr_core::types::enums::FileFormat;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ComicMetadata {
    pub title: Option<String>,
    pub series: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub page_count: Option<u32>,
    pub writer: Option<String>,
    pub artist: Option<String>,
    pub year: Option<String>,
    pub cover_data: Option<Vec<u8>>,
}

pub fn extract_comic_metadata(
    path: &Path,
    format: FileFormat,
) -> Result<ComicMetadata, ScannerError> {
    match format {
        FileFormat::Cbz => extract_cbz(path),
        FileFormat::Cbr => Err(ScannerError::UnsupportedFormat(
            "CBR extraction requires unrar library".into(),
        )),
        _ => Err(ScannerError::UnsupportedFormat(format!("{format:?}"))),
    }
}

fn extract_cbz(path: &Path) -> Result<ComicMetadata, ScannerError> {
    let file = std::fs::File::open(path).map_err(|e| ScannerError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ScannerError::ExtractionFailed {
        path: path.to_path_buf(),
        reason: format!("not a valid ZIP: {e}"),
    })?;

    let mut meta = try_parse_comicinfo(&mut archive, path);
    if meta.cover_data.is_none() {
        meta.cover_data = extract_first_image(&mut archive);
    }

    if meta.title.is_none() {
        meta = apply_filename_fallback(path, meta);
    }

    Ok(meta)
}

fn try_parse_comicinfo(
    archive: &mut zip::ZipArchive<std::fs::File>,
    epub_path: &Path,
) -> ComicMetadata {
    let xml = match archive.by_name("ComicInfo.xml") {
        Ok(mut entry) => {
            let mut contents = String::new();
            if entry.read_to_string(&mut contents).is_err() {
                return ComicMetadata::default();
            }
            contents
        }
        Err(_) => return ComicMetadata::default(),
    };

    parse_comicinfo_xml(&xml, epub_path)
}

fn parse_comicinfo_xml(xml: &str, _path: &Path) -> ComicMetadata {
    let mut meta = ComicMetadata::default();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                current_tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
            }
            Ok(Event::Text(ref t)) => {
                let text = t.unescape().unwrap_or_default().trim().to_string();
                if !text.is_empty() {
                    match current_tag.as_str() {
                        "Title" => meta.title = Some(text),
                        "Series" => meta.series = Some(text),
                        "Volume" => meta.volume = Some(text),
                        "Number" => meta.issue = Some(text),
                        "PageCount" => meta.page_count = text.parse().ok(),
                        "Writer" => meta.writer = Some(text),
                        "Penciller" | "Artist" => meta.artist = Some(text),
                        "Year" => meta.year = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(_)) => current_tag.clear(),
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    meta
}

fn extract_first_image(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<Vec<u8>> {
    let mut image_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive.by_index(i).ok().and_then(|e| {
                let name = e.name().to_lowercase();
                if name.ends_with(".jpg")
                    || name.ends_with(".jpeg")
                    || name.ends_with(".png")
                    || name.ends_with(".webp")
                {
                    Some(e.name().to_string())
                } else {
                    None
                }
            })
        })
        .collect();
    image_names.sort();

    let first = image_names.first()?;
    let mut entry = archive.by_name(first).ok()?;
    let mut data = Vec::new();
    entry.read_to_end(&mut data).ok()?;
    Some(data)
}

fn apply_filename_fallback(path: &Path, mut meta: ComicMetadata) -> ComicMetadata {
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        meta.title = Some(stem.to_string());
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_cbz_with_comicinfo(xml: &str) -> tempfile::NamedTempFile {
        let f = tempfile::Builder::new().suffix(".cbz").tempfile().unwrap();
        {
            let file = std::fs::File::create(f.path()).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("ComicInfo.xml", opts).unwrap();
            zip.write_all(xml.as_bytes()).unwrap();

            zip.start_file("page001.jpg", opts).unwrap();
            zip.write_all(b"\xff\xd8\xff\xe0fake-jpeg-data").unwrap();

            zip.finish().unwrap();
        }
        f
    }

    #[test]
    fn extract_from_comicinfo_xml() {
        let xml = r#"<?xml version="1.0"?>
<ComicInfo>
  <Title>Batman #1</Title>
  <Series>Batman</Series>
  <Number>1</Number>
  <Volume>2016</Volume>
  <Writer>Tom King</Writer>
  <PageCount>24</PageCount>
  <Year>2016</Year>
</ComicInfo>"#;
        let cbz = create_cbz_with_comicinfo(xml);
        let meta = extract_comic_metadata(cbz.path(), FileFormat::Cbz).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Batman #1"));
        assert_eq!(meta.series.as_deref(), Some("Batman"));
        assert_eq!(meta.issue.as_deref(), Some("1"));
        assert_eq!(meta.writer.as_deref(), Some("Tom King"));
        assert_eq!(meta.page_count, Some(24));
    }

    #[test]
    fn cbz_without_comicinfo_uses_filename() {
        let f = tempfile::Builder::new()
            .prefix("My Comic 001")
            .suffix(".cbz")
            .tempfile()
            .unwrap();
        {
            let file = std::fs::File::create(f.path()).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("page001.jpg", opts).unwrap();
            zip.write_all(b"\xff\xd8\xff\xe0fake").unwrap();
            zip.finish().unwrap();
        }
        let meta = extract_comic_metadata(f.path(), FileFormat::Cbz).unwrap();
        assert!(meta.title.is_some());
    }

    #[test]
    fn cbz_extracts_cover_image() {
        let xml = "<ComicInfo><Title>Test</Title></ComicInfo>";
        let cbz = create_cbz_with_comicinfo(xml);
        let meta = extract_comic_metadata(cbz.path(), FileFormat::Cbz).unwrap();
        assert!(meta.cover_data.is_some());
    }
}
