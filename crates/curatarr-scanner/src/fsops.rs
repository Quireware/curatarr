//! Small filesystem helpers shared by the import pipeline and the recycle bin.

use curatarr_core::error::ScannerError;
use std::path::Path;

pub(crate) fn io_err(path: &Path, source: std::io::Error) -> ScannerError {
    ScannerError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Move a file, creating the destination's parent and falling back to copy + delete when a
/// rename is not possible (e.g. across filesystems). Refuses to overwrite an existing file.
pub(crate) async fn move_file(source: &Path, dest: &Path) -> Result<(), ScannerError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| io_err(parent, e))?;
    }
    if tokio::fs::try_exists(dest).await.unwrap_or(false) {
        return Err(io_err(
            dest,
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "destination already exists",
            ),
        ));
    }
    if tokio::fs::rename(source, dest).await.is_ok() {
        return Ok(());
    }
    tokio::fs::copy(source, dest)
        .await
        .map_err(|e| io_err(dest, e))?;
    tokio::fs::remove_file(source)
        .await
        .map_err(|e| io_err(source, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn move_file_creates_parent_and_moves() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("deep/nested/b.txt");
        std::fs::write(&src, b"hello").unwrap();
        move_file(&src, &dst).await.unwrap();
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn move_file_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        std::fs::write(&src, b"a").unwrap();
        std::fs::write(&dst, b"b").unwrap();
        assert!(move_file(&src, &dst).await.is_err());
        assert_eq!(std::fs::read(&dst).unwrap(), b"b");
        assert!(src.exists());
    }
}
