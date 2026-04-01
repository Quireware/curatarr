use curatarr_core::error::ScannerError;
use curatarr_core::types::enums::FileFormat;
use std::path::Path;

const MAGIC_LEN: usize = 512;

pub fn detect_format(path: &Path) -> Result<FileFormat, ScannerError> {
    let buf = read_magic(path)?;
    detect_from_magic(&buf, path)
        .or_else(|| detect_from_extension(path))
        .ok_or_else(|| {
            ScannerError::UnsupportedFormat(
                path.extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown".into()),
            )
        })
}

fn read_magic(path: &Path) -> Result<Vec<u8>, ScannerError> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| ScannerError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut buf = vec![0u8; MAGIC_LEN];
    let n = file.read(&mut buf).map_err(|e| ScannerError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    buf.truncate(n);
    Ok(buf)
}

fn detect_from_magic(buf: &[u8], path: &Path) -> Option<FileFormat> {
    if buf.len() < 4 {
        return None;
    }

    if buf.starts_with(b"%PDF-") {
        return Some(FileFormat::Pdf);
    }

    if buf.starts_with(b"PK\x03\x04") {
        return detect_zip_format(path);
    }

    if buf.starts_with(b"Rar!\x1a\x07") {
        return Some(FileFormat::Cbr);
    }

    if buf.len() >= 8 && buf.starts_with(b"AT&TFORM") {
        return Some(FileFormat::Djvu);
    }

    if buf.len() >= 8 && &buf[..8] == b"BOOKMOBI" {
        return Some(FileFormat::Mobi);
    }

    if is_fb2(buf) {
        return Some(FileFormat::Fb2);
    }

    None
}

fn detect_zip_format(path: &Path) -> Option<FileFormat> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_lowercase()))
        .collect();

    if let Some(idx) = names.iter().position(|n| n == "mimetype") {
        let mut contents = String::new();
        use std::io::Read;
        if let Ok(mut entry) = archive.by_index(idx) {
            let _ = entry.read_to_string(&mut contents);
            if contents.trim() == "application/epub+zip" {
                return Some(FileFormat::Epub);
            }
        }
    }

    if names.iter().any(|n| n == "meta-inf/container.xml") {
        return Some(FileFormat::Epub);
    }

    if has_image_files(&names) {
        return Some(FileFormat::Cbz);
    }

    None
}

fn has_image_files(names: &[String]) -> bool {
    names.iter().any(|name| {
        name.ends_with(".jpg")
            || name.ends_with(".jpeg")
            || name.ends_with(".png")
            || name.ends_with(".webp")
    })
}

fn is_fb2(buf: &[u8]) -> bool {
    let text = String::from_utf8_lossy(buf);
    let trimmed = text.trim_start();
    trimmed.starts_with("<?xml") && trimmed.contains("<FictionBook")
}

fn detect_from_extension(path: &Path) -> Option<FileFormat> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "epub" => Some(FileFormat::Epub),
        "mobi" => Some(FileFormat::Mobi),
        "azw3" | "azw" => Some(FileFormat::Azw3),
        "cbz" => Some(FileFormat::Cbz),
        "cbr" => Some(FileFormat::Cbr),
        "cb7" => Some(FileFormat::Cb7),
        "cbt" => Some(FileFormat::Cbt),
        "pdf" => Some(FileFormat::Pdf),
        "djvu" | "djv" => Some(FileFormat::Djvu),
        "fb2" => Some(FileFormat::Fb2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;
    use std::io::Write;

    fn write_temp(data: &[u8], ext: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .unwrap();
        f.write_all(data).unwrap();
        f.flush().unwrap();
        f
    }

    #[rstest]
    #[case(b"%PDF-1.7\n", "pdf", FileFormat::Pdf)]
    #[case(b"Rar!\x1a\x07\x00", "cbr", FileFormat::Cbr)]
    #[case(b"AT&TFORM\x00\x00\x00\x00DJVU", "djvu", FileFormat::Djvu)]
    #[case(b"BOOKMOBI\x00\x00\x00\x00", "mobi", FileFormat::Mobi)]
    fn magic_bytes_detect_format(
        #[case] data: &[u8],
        #[case] ext: &str,
        #[case] expected: FileFormat,
    ) {
        let f = write_temp(data, ext);
        let result = detect_format(f.path()).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn fb2_detected_from_xml_content() {
        let data = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\">";
        let f = write_temp(data, "fb2");
        let result = detect_format(f.path()).unwrap();
        assert_eq!(result, FileFormat::Fb2);
    }

    #[rstest]
    #[case("epub", FileFormat::Epub)]
    #[case("mobi", FileFormat::Mobi)]
    #[case("azw3", FileFormat::Azw3)]
    #[case("cbz", FileFormat::Cbz)]
    #[case("cbr", FileFormat::Cbr)]
    #[case("cb7", FileFormat::Cb7)]
    #[case("cbt", FileFormat::Cbt)]
    #[case("pdf", FileFormat::Pdf)]
    #[case("djvu", FileFormat::Djvu)]
    #[case("fb2", FileFormat::Fb2)]
    fn extension_fallback(#[case] ext: &str, #[case] expected: FileFormat) {
        let f = write_temp(b"not magic bytes", ext);
        let result = detect_format(f.path()).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn unsupported_extension_returns_error() {
        let f = write_temp(b"hello world", "txt");
        assert!(detect_format(f.path()).is_err());
    }

    #[test]
    fn nonexistent_file_returns_io_error() {
        let result = detect_format(Path::new("/nonexistent/file.epub"));
        assert!(result.is_err());
    }

    proptest! {
        #[test]
        fn random_bytes_do_not_panic(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let f = write_temp(&data, "bin");
            let _ = detect_format(f.path());
        }
    }
}
