use crate::score::{format_position, format_rank};
use curatarr_core::types::enums::FileFormat;
use curatarr_core::types::profile::QualityProfile;
use curatarr_core::types::release::Release;

/// True when `release` ranks strictly better than every format already on disk.
pub fn should_upgrade(
    existing: &[FileFormat],
    release: &Release,
    profile: &QualityProfile,
) -> bool {
    if let Some(max) = profile.max_size_bytes {
        if release.size_bytes > max {
            return false;
        }
    }
    let new_rank = format_rank(release, &profile.format_order);
    if existing.is_empty() {
        return new_rank != usize::MAX;
    }
    let best_existing = existing
        .iter()
        .map(|fmt| format_position(*fmt, &profile.format_order))
        .min()
        .unwrap_or(usize::MAX);
    new_rank < best_existing
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_core::traits::indexer::IndexerProtocol;
    use rstest::rstest;
    use url::Url;

    fn rel(title: &str) -> Release {
        Release {
            title: title.into(),
            guid: title.into(),
            indexer: "x".into(),
            download_url: Url::parse("http://x/file").unwrap(),
            size_bytes: 10,
            protocol: IndexerProtocol::Newznab,
            info_hash: None,
        }
    }

    fn profile(order: Vec<FileFormat>) -> QualityProfile {
        QualityProfile {
            id: curatarr_core::types::id::QualityProfileId::new(),
            name: "t".into(),
            format_order: order,
            max_size_bytes: None,
        }
    }

    #[rstest]
    #[case(FileFormat::Pdf, "Dune EPUB", true)]
    #[case(FileFormat::Epub, "Dune PDF", false)]
    fn upgrade_follows_profile_order(
        #[case] on_disk: FileFormat,
        #[case] release_title: &str,
        #[case] upgrade: bool,
    ) {
        let p = profile(vec![FileFormat::Epub, FileFormat::Pdf]);
        assert_eq!(should_upgrade(&[on_disk], &rel(release_title), &p), upgrade);
    }
}
