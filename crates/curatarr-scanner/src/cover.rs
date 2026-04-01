use curatarr_core::error::ScannerError;
use image::DynamicImage;
use image::imageops::FilterType;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct CoverPaths {
    pub thumb_64: PathBuf,
    pub medium_256: PathBuf,
    pub large_1024: PathBuf,
    pub original: PathBuf,
}

pub fn process_cover(image_bytes: &[u8], storage_dir: &Path) -> Result<CoverPaths, ScannerError> {
    let hash = format!("{:x}", Sha256::digest(image_bytes));
    let cover_dir = storage_dir.join(&hash);

    std::fs::create_dir_all(&cover_dir).map_err(|e| ScannerError::Io {
        path: cover_dir.clone(),
        source: e,
    })?;

    let img = image::load_from_memory(image_bytes).map_err(|e| ScannerError::ExtractionFailed {
        path: cover_dir.clone(),
        reason: format!("failed to decode image: {e}"),
    })?;

    let original_path = cover_dir.join("original.jpg");
    save_jpeg(&img, &original_path)?;

    let thumb_path = cover_dir.join("thumb_64.jpg");
    save_resized(&img, 64, &thumb_path)?;

    let medium_path = cover_dir.join("medium_256.jpg");
    save_resized(&img, 256, &medium_path)?;

    let large_path = cover_dir.join("large_1024.jpg");
    save_resized(&img, 1024, &large_path)?;

    Ok(CoverPaths {
        thumb_64: thumb_path,
        medium_256: medium_path,
        large_1024: large_path,
        original: original_path,
    })
}

fn save_jpeg(img: &DynamicImage, path: &Path) -> Result<(), ScannerError> {
    img.save(path).map_err(|e| ScannerError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(e.to_string()),
    })
}

fn save_resized(img: &DynamicImage, max_height: u32, path: &Path) -> Result<(), ScannerError> {
    let resized = if img.height() > max_height {
        img.resize(u32::MAX, max_height, FilterType::Lanczos3)
    } else {
        img.clone() // clone: image crate requires owned value for save
    };
    save_jpeg(&resized, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn create_test_image(width: u32, height: u32) -> Vec<u8> {
        let img = DynamicImage::new_rgb8(width, height);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn process_cover_creates_all_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let img_data = create_test_image(2000, 3000);

        let paths = process_cover(&img_data, dir.path()).unwrap();

        assert!(paths.original.exists());
        assert!(paths.thumb_64.exists());
        assert!(paths.medium_256.exists());
        assert!(paths.large_1024.exists());
    }

    #[test]
    fn thumbnail_dimensions_correct() {
        let dir = tempfile::tempdir().unwrap();
        let img_data = create_test_image(800, 1200);

        let paths = process_cover(&img_data, dir.path()).unwrap();

        let thumb = image::open(&paths.thumb_64).unwrap();
        assert!(thumb.height() <= 64);
        assert!(thumb.width() > 0);

        let medium = image::open(&paths.medium_256).unwrap();
        assert!(medium.height() <= 256);
    }

    #[test]
    fn content_addressed_directory_uses_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let img_data = create_test_image(100, 100);

        let hash = format!("{:x}", Sha256::digest(&img_data));
        let paths = process_cover(&img_data, dir.path()).unwrap();

        assert!(paths.original.to_string_lossy().contains(&hash));
    }

    #[test]
    fn invalid_image_data_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = process_cover(b"not an image", dir.path());
        assert!(result.is_err());
    }

    proptest! {
        #[test]
        fn resizing_never_produces_zero_dimensions(
            w in 1u32..500,
            h in 1u32..500,
        ) {
            let img = DynamicImage::new_rgb8(w, h);
            let resized = img.resize(u32::MAX, 64, FilterType::Lanczos3);
            prop_assert!(resized.width() > 0);
            prop_assert!(resized.height() > 0);
        }
    }
}
