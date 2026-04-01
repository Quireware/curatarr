use curatarr_core::error::ScannerError;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

const BUF_SIZE: usize = 8192;

pub async fn hash_file(path: &Path) -> Result<String, ScannerError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| ScannerError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF_SIZE];

    loop {
        let n = file.read(&mut buf).await.map_err(|e| ScannerError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;
    use std::io::Write;

    fn write_temp(data: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(data).unwrap();
        f.flush().unwrap();
        f
    }

    #[rstest]
    #[case(
        b"",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    )]
    #[case(
        b"hello world",
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    )]
    #[tokio::test]
    async fn known_test_vectors(#[case] data: &[u8], #[case] expected: &str) {
        let f = write_temp(data);
        let hash = hash_file(f.path()).await.unwrap();
        assert_eq!(hash, expected);
    }

    #[tokio::test]
    async fn hashing_is_deterministic() {
        let f = write_temp(b"deterministic input");
        let h1 = hash_file(f.path()).await.unwrap();
        let h2 = hash_file(f.path()).await.unwrap();
        assert_eq!(h1, h2);
    }

    proptest! {
        #[test]
        fn hash_deterministic_sync(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let f = write_temp(&data);
            let h1 = rt.block_on(hash_file(f.path())).unwrap();
            let h2 = rt.block_on(hash_file(f.path())).unwrap();
            prop_assert_eq!(h1, h2);
        }

        #[test]
        fn different_inputs_different_hashes(
            a in proptest::collection::vec(any::<u8>(), 1..256),
            b in proptest::collection::vec(any::<u8>(), 1..256),
        ) {
            prop_assume!(a != b);
            let rt = tokio::runtime::Runtime::new().unwrap();
            let fa = write_temp(&a);
            let fb = write_temp(&b);
            let ha = rt.block_on(hash_file(fa.path())).unwrap();
            let hb = rt.block_on(hash_file(fb.path())).unwrap();
            prop_assert_ne!(ha, hb);
        }
    }

    #[tokio::test]
    async fn nonexistent_file_returns_error() {
        let result = hash_file(Path::new("/nonexistent/file")).await;
        assert!(result.is_err());
    }
}
