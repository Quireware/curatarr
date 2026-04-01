use curatarr_core::error::ScannerError;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: Option<u32>,
    pub creation_date: Option<String>,
}

pub fn extract_pdf_metadata(path: &Path) -> Result<PdfMetadata, ScannerError> {
    let doc = lopdf::Document::load(path).map_err(|e| ScannerError::ExtractionFailed {
        path: path.to_path_buf(),
        reason: format!("failed to parse PDF: {e}"),
    })?;

    let mut meta = PdfMetadata {
        page_count: Some(u32::try_from(doc.get_pages().len()).unwrap_or(0)),
        ..Default::default()
    };

    if let Ok(info_id) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_id.as_reference() {
            if let Ok(info) = doc.get_dictionary(info_ref) {
                meta.title = get_pdf_string(info, b"Title");
                meta.author = get_pdf_string(info, b"Author");
                meta.creation_date = get_pdf_string(info, b"CreationDate");
            }
        }
    }

    Ok(meta)
}

fn get_pdf_string(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|v| match v {
        lopdf::Object::String(bytes, _) => {
            let s = String::from_utf8_lossy(bytes).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonexistent_pdf_returns_error() {
        let result = extract_pdf_metadata(Path::new("/nonexistent/file.pdf"));
        assert!(result.is_err());
    }

    #[test]
    fn non_pdf_file_returns_error() {
        let mut f = tempfile::NamedTempFile::with_suffix(".pdf").unwrap();
        use std::io::Write;
        f.write_all(b"not a pdf file").unwrap();
        let result = extract_pdf_metadata(f.path());
        assert!(result.is_err());
    }
}
