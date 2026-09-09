use curatarr_core::types::enums::FileFormat;
use curatarr_core::types::profile::QualityProfile;
use curatarr_core::types::release::Release;

pub fn pick<'a>(releases: &'a [Release], profile: &QualityProfile) -> Option<&'a Release> {
    ranked(releases, profile).into_iter().next()
}

/// Releases that pass the size cap, best format first, then smaller size.
pub fn ranked<'a>(releases: &'a [Release], profile: &QualityProfile) -> Vec<&'a Release> {
    let mut scored: Vec<(&Release, usize, u64)> = releases
        .iter()
        .filter(|release| match profile.max_size_bytes {
            Some(max) => release.size_bytes <= max,
            None => true,
        })
        .map(|release| {
            (
                release,
                format_rank(release, &profile.format_order),
                release.size_bytes,
            )
        })
        .collect();
    scored.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
    scored.into_iter().map(|(r, _, _)| r).collect()
}

pub(crate) fn format_rank(release: &Release, order: &[FileFormat]) -> usize {
    detect_format(release)
        .map(|fmt| format_position(fmt, order))
        .unwrap_or(usize::MAX)
}

pub(crate) fn format_position(fmt: FileFormat, order: &[FileFormat]) -> usize {
    order.iter().position(|f| *f == fmt).unwrap_or(usize::MAX)
}

fn detect_format(release: &Release) -> Option<FileFormat> {
    let hay = format!("{} {}", release.title, release.download_url.path()).to_ascii_lowercase();
    if hay.contains("epub") {
        Some(FileFormat::Epub)
    } else if hay.contains("azw") {
        Some(FileFormat::Azw3)
    } else if hay.contains("mobi") {
        Some(FileFormat::Mobi)
    } else if hay.contains(".cbz") || hay.contains(" cbz") {
        Some(FileFormat::Cbz)
    } else if hay.contains(".cbr") || hay.contains(" cbr") {
        Some(FileFormat::Cbr)
    } else if hay.contains("pdf") {
        Some(FileFormat::Pdf)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_core::traits::indexer::IndexerProtocol;
    use rstest::rstest;
    use url::Url;

    fn rel(title: &str, size: u64) -> Release {
        Release {
            title: title.into(),
            guid: title.into(),
            indexer: "x".into(),
            download_url: Url::parse("http://x/file").unwrap(),
            size_bytes: size,
            protocol: IndexerProtocol::Newznab,
            info_hash: None,
        }
    }

    fn profile(order: Vec<FileFormat>, max: Option<u64>) -> QualityProfile {
        QualityProfile {
            id: curatarr_core::types::id::QualityProfileId::new(),
            name: "t".into(),
            format_order: order,
            max_size_bytes: max,
        }
    }

    #[rstest]
    #[case("Dune EPUB", "Dune PDF")]
    fn epub_beats_pdf(#[case] a: &str, #[case] b: &str) {
        let releases = [rel(a, 10), rel(b, 5)];
        let p = profile(vec![FileFormat::Epub, FileFormat::Pdf], None);
        assert_eq!(pick(&releases, &p).unwrap().title, "Dune EPUB");
    }

    #[test]
    fn oversize_is_rejected() {
        let releases = [rel("Dune EPUB", 1000)];
        let p = profile(vec![FileFormat::Epub], Some(100));
        assert!(pick(&releases, &p).is_none());
    }

    #[test]
    fn unknown_format_ranks_last() {
        let releases = [rel("Dune.bin", 1), rel("Dune PDF", 10)];
        let p = profile(vec![FileFormat::Pdf], None);
        assert_eq!(pick(&releases, &p).unwrap().title, "Dune PDF");
    }
}
