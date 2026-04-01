use curatarr_core::error::ScannerError;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct EpubMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub pub_date: Option<String>,
    pub cover_path: Option<String>,
}

pub fn extract_epub_metadata(path: &Path) -> Result<EpubMetadata, ScannerError> {
    let file = std::fs::File::open(path).map_err(|e| ScannerError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ScannerError::ExtractionFailed {
        path: path.to_path_buf(),
        reason: format!("not a valid ZIP: {e}"),
    })?;

    let opf_path = find_opf_path(&mut archive, path)?;
    let opf_xml = read_zip_entry(&mut archive, &opf_path, path)?;
    parse_opf(&opf_xml, path)
}

fn find_opf_path(
    archive: &mut zip::ZipArchive<std::fs::File>,
    epub_path: &Path,
) -> Result<String, ScannerError> {
    let container_xml = read_zip_entry(archive, "META-INF/container.xml", epub_path)?;
    parse_container_xml(&container_xml, epub_path)
}

fn read_zip_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    entry_name: &str,
    epub_path: &Path,
) -> Result<String, ScannerError> {
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|_| ScannerError::ExtractionFailed {
            path: epub_path.to_path_buf(),
            reason: format!("missing {entry_name}"),
        })?;
    let mut contents = String::new();
    entry
        .read_to_string(&mut contents)
        .map_err(|e| ScannerError::ExtractionFailed {
            path: epub_path.to_path_buf(),
            reason: format!("failed to read {entry_name}: {e}"),
        })?;
    Ok(contents)
}

fn parse_container_xml(xml: &str, epub_path: &Path) -> Result<String, ScannerError> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
                if e.local_name().as_ref() == b"rootfile" =>
            {
                for attr in e.attributes().flatten() {
                    if attr.key.local_name().as_ref() == b"full-path" {
                        return Ok(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ScannerError::ExtractionFailed {
                    path: epub_path.to_path_buf(),
                    reason: format!("XML parse error in container.xml: {e}"),
                });
            }
            _ => {}
        }
        buf.clear();
    }

    Err(ScannerError::ExtractionFailed {
        path: epub_path.to_path_buf(),
        reason: "no rootfile found in container.xml".into(),
    })
}

fn parse_opf(xml: &str, epub_path: &Path) -> Result<EpubMetadata, ScannerError> {
    let mut meta = EpubMetadata::default();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current_tag = String::new();
    let mut in_metadata = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                let name = String::from_utf8_lossy(local.as_ref()).into_owned();

                if name == "metadata" {
                    in_metadata = true;
                }
                if in_metadata {
                    current_tag = name;
                }

                if local.as_ref() == b"meta" {
                    handle_meta_element(e, &mut meta);
                }
                if local.as_ref() == b"item" {
                    handle_item_element(e, &mut meta);
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"meta" {
                    handle_meta_element(e, &mut meta);
                }
                if e.local_name().as_ref() == b"item" {
                    handle_item_element(e, &mut meta);
                }
            }
            Ok(Event::Text(ref t)) if in_metadata => {
                let text = t.unescape().unwrap_or_default().trim().to_string();
                if !text.is_empty() {
                    match current_tag.as_str() {
                        "title" => meta.title = Some(text),
                        "creator" => meta.authors.push(text),
                        "language" => meta.language = Some(text),
                        "description" => meta.description = Some(text),
                        "publisher" => meta.publisher = Some(text),
                        "date" => meta.pub_date = Some(text),
                        "identifier" => {
                            if looks_like_isbn(&text) {
                                meta.isbn = Some(text);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if e.local_name().as_ref() == b"metadata" {
                    in_metadata = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ScannerError::ExtractionFailed {
                    path: epub_path.to_path_buf(),
                    reason: format!("XML parse error in OPF: {e}"),
                });
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(meta)
}

fn handle_meta_element(e: &quick_xml::events::BytesStart<'_>, meta: &mut EpubMetadata) {
    let mut name_val = None;
    let mut content_val = None;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match key.as_str() {
            "name" => name_val = Some(val),
            "content" => content_val = Some(val),
            _ => {}
        }
    }

    if let (Some(name), Some(content)) = (name_val, content_val) {
        if name == "cover" {
            meta.cover_path = Some(content);
        }
    }
}

fn handle_item_element(e: &quick_xml::events::BytesStart<'_>, meta: &mut EpubMetadata) {
    let mut id = None;
    let mut href = None;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match key.as_str() {
            "id" => id = Some(val),
            "href" => href = Some(val),
            _ => {}
        }
    }

    if let (Some(item_id), Some(item_href)) = (id, href) {
        if let Some(cover_id) = &meta.cover_path {
            if &item_id == cover_id {
                meta.cover_path = Some(item_href);
            }
        }
    }
}

fn looks_like_isbn(s: &str) -> bool {
    let digits: Vec<char> = s.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.len() == 10 || digits.len() == 13
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Write;

    fn create_minimal_epub(title: &str, author: &str) -> tempfile::NamedTempFile {
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest/>
  <spine/>
</package>"#
        );

        let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles>
    <rootfile full-path="content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

        let f = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
        {
            let file = std::fs::File::create(f.path()).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("mimetype", opts).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();

            zip.start_file("META-INF/container.xml", opts).unwrap();
            zip.write_all(container.as_bytes()).unwrap();

            zip.start_file("content.opf", opts).unwrap();
            zip.write_all(opf.as_bytes()).unwrap();

            zip.finish().unwrap();
        }
        f
    }

    #[test]
    fn extract_basic_metadata() {
        let epub = create_minimal_epub("Dune", "Frank Herbert");
        let meta = extract_epub_metadata(epub.path()).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Dune"));
        assert_eq!(meta.authors, vec!["Frank Herbert"]);
        assert_eq!(meta.language.as_deref(), Some("en"));
    }

    #[test]
    fn nonexistent_file_returns_error() {
        let result = extract_epub_metadata(Path::new("/nonexistent/book.epub"));
        assert!(result.is_err());
    }

    #[test]
    fn non_zip_file_returns_error() {
        let mut f = tempfile::NamedTempFile::with_suffix(".epub").unwrap();
        f.write_all(b"not a zip file").unwrap();
        let result = extract_epub_metadata(f.path());
        assert!(result.is_err());
    }

    proptest! {
        #[test]
        fn malformed_xml_does_not_panic(data in "\\PC{0,200}") {
            let _ = parse_opf(&data, Path::new("test.epub"));
        }

        #[test]
        fn malformed_container_does_not_panic(data in "\\PC{0,200}") {
            let _ = parse_container_xml(&data, Path::new("test.epub"));
        }
    }

    #[test]
    fn isbn_detection() {
        assert!(looks_like_isbn("978-0-306-40615-7"));
        assert!(looks_like_isbn("9780306406157"));
        assert!(looks_like_isbn("0-306-40615-2"));
        assert!(!looks_like_isbn("not-an-isbn"));
        assert!(!looks_like_isbn("12345"));
    }
}
