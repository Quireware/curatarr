use curatarr_core::error::ScannerError;
use curatarr_core::types::enums::FileFormat;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

use crate::format::detect_format;

/// A supported book/comic file found during a directory walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub format: FileFormat,
    pub size: u64,
}

/// Walk `root` recursively, returning supported book/comic files sorted by path.
///
/// Each exclusion pattern is matched against both the entry's file name and its
/// path relative to `root`, so `*.tmp`, `.DS_Store`, `._*`, `drafts` and
/// `**/drafts/**` all behave as users expect. Excluded directories are pruned
/// and never descended into. Unsupported files are skipped silently; files
/// that cannot be read are logged and skipped. A single bad file never aborts
/// the walk.
pub fn scan_directory(
    root: &Path,
    exclusions: &[String],
) -> Result<Vec<DiscoveredFile>, ScannerError> {
    validate_root(root)?;
    let globs = build_globset(exclusions)?;

    let mut found = Vec::new();

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_excluded(entry, root, &globs));

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "skipping unreadable directory entry");
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        if let Some(file) = discover(&entry) {
            found.push(file);
        }
    }

    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

fn validate_root(root: &Path) -> Result<(), ScannerError> {
    let meta = std::fs::metadata(root).map_err(|e| ScannerError::Io {
        path: root.to_path_buf(),
        source: e,
    })?;
    if !meta.is_dir() {
        return Err(ScannerError::Io {
            path: root.to_path_buf(),
            source: std::io::Error::other("scan root is not a directory"),
        });
    }
    Ok(())
}

fn build_globset(exclusions: &[String]) -> Result<GlobSet, ScannerError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in exclusions {
        let glob = Glob::new(pattern).map_err(|e| ScannerError::InvalidExclusion {
            pattern: pattern.to_string(),
            reason: e.to_string(),
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|e| ScannerError::InvalidExclusion {
        pattern: exclusions.join(", "),
        reason: e.to_string(),
    })
}

fn is_excluded(entry: &DirEntry, root: &Path, globs: &GlobSet) -> bool {
    if globs.is_empty() || entry.depth() == 0 {
        return false;
    }

    if globs.is_match(entry.file_name()) {
        return true;
    }

    entry
        .path()
        .strip_prefix(root)
        .map(|rel| globs.is_match(rel))
        .unwrap_or(false)
}

fn discover(entry: &DirEntry) -> Option<DiscoveredFile> {
    let path = entry.path();

    let format = match detect_format(path) {
        Ok(f) => f,
        Err(ScannerError::UnsupportedFormat(_)) => return None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "skipping file");
            return None;
        }
    };

    let size = match entry.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "skipping file: metadata unreadable");
            return None;
        }
    };

    Some(DiscoveredFile {
        path: path.to_path_buf(),
        format,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;
    use std::fs;

    fn write(root: &Path, rel: &str, data: &[u8]) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, data).unwrap();
    }

    fn rel_paths(root: &Path, files: &[DiscoveredFile]) -> Vec<String> {
        files
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[rstest]
    fn finds_only_supported_files_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "sub/deep/c.pdf", b"not magic");
        write(root, "notes.txt", b"hello");
        write(root, "sub/b.cbz", b"not magic");
        write(root, "image.jpg", b"jpeg-ish");
        write(root, "a.epub", b"not magic");

        let found = scan_directory(root, &[]).unwrap();

        assert_eq!(
            rel_paths(root, &found),
            vec!["a.epub", "sub/b.cbz", "sub/deep/c.pdf"]
        );
        assert_eq!(found[0].format, FileFormat::Epub);
        assert_eq!(found[1].format, FileFormat::Cbz);
        assert_eq!(found[2].format, FileFormat::Pdf);
        assert!(found.iter().all(|f| f.size == 9));
    }

    #[rstest]
    fn exclusion_patterns_skip_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "._a.epub", b"resource fork");
        write(root, ".DS_Store", b"finder");
        write(root, "x.epub.tmp", b"partial");
        write(root, "keep.epub", b"not magic");
        write(root, "nested/._b.cbz", b"resource fork");
        write(root, "nested/keep.cbz", b"not magic");

        let exclusions = vec!["._*".to_string(), ".DS_Store".into(), "*.tmp".into()];
        let found = scan_directory(root, &exclusions).unwrap();

        assert_eq!(
            rel_paths(root, &found),
            vec!["keep.epub", "nested/keep.cbz"]
        );
    }

    #[rstest]
    #[case("drafts")]
    #[case("**/drafts/**")]
    fn directory_exclusion_prunes_subtree(#[case] pattern: &str) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "drafts/d.epub", b"not magic");
        write(root, "drafts/inner/f.epub", b"not magic");
        write(root, "final/e.epub", b"not magic");

        let found = scan_directory(root, &[pattern.to_string()]).unwrap();

        assert_eq!(rel_paths(root, &found), vec!["final/e.epub"]);
    }

    #[rstest]
    fn empty_directory_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let found = scan_directory(dir.path(), &[]).unwrap();
        assert!(found.is_empty());
    }

    #[rstest]
    fn nonexistent_root_returns_io_error() {
        let result = scan_directory(Path::new("/nonexistent/curatarr/scan/root"), &[]);
        assert!(matches!(result, Err(ScannerError::Io { .. })));
    }

    #[rstest]
    fn file_as_root_returns_io_error() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.epub", b"not magic");
        let result = scan_directory(&dir.path().join("a.epub"), &[]);
        assert!(matches!(result, Err(ScannerError::Io { .. })));
    }

    #[rstest]
    fn invalid_glob_returns_invalid_exclusion() {
        let dir = tempfile::tempdir().unwrap();
        let result = scan_directory(dir.path(), &["[unclosed".to_string()]);
        assert!(matches!(
            result,
            Err(ScannerError::InvalidExclusion { ref pattern, .. }) if pattern == "[unclosed"
        ));
    }

    proptest! {
        #[test]
        fn random_exclusions_never_panic(
            exclusions in proptest::collection::vec("[a-z.*]{1,8}", 0..5)
        ) {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "a.epub", b"not magic");
            write(dir.path(), "sub/b.cbz", b"not magic");
            let _ = scan_directory(dir.path(), &exclusions);
        }
    }
}
