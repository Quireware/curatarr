//! Duplicate detection: exact matches by SHA-256 and near matches by title similarity.

use curatarr_core::error::ScannerError;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::file::{FileFilter, LibraryFile};
use curatarr_core::types::work::{Work, WorkFilter};
use serde::Serialize;
use std::collections::BTreeMap;

pub const DEFAULT_NEAR_THRESHOLD: f64 = 0.85;
const PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicateGroup {
    pub sha256: String,
    pub files: Vec<LibraryFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NearDuplicate {
    pub work_a: Work,
    pub work_b: Work,
    pub similarity: f64,
}

/// Groups of two or more non-deleted library files sharing a content hash.
pub async fn find_duplicates(db: &dyn Repository) -> Result<Vec<DuplicateGroup>, ScannerError> {
    let mut by_hash: BTreeMap<String, Vec<LibraryFile>> = BTreeMap::new();
    let mut page = 1;
    loop {
        let batch = db
            .list_files(
                &FileFilter::default(),
                &Pagination {
                    page,
                    per_page: PAGE_SIZE,
                },
            )
            .await?;
        let fetched = batch.items.len();
        for file in batch.items {
            by_hash
                .entry(file.sha256.clone()) // clone: hash is both the map key and a field
                .or_default()
                .push(file);
        }
        if fetched < usize::try_from(PAGE_SIZE).unwrap_or(usize::MAX) {
            break;
        }
        page += 1;
    }

    Ok(by_hash
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(sha256, files)| DuplicateGroup { sha256, files })
        .collect())
}

/// Pairs of works whose normalised titles are at least `threshold` similar (0.0..=1.0).
pub async fn find_near_duplicates(
    db: &dyn Repository,
    threshold: f64,
) -> Result<Vec<NearDuplicate>, ScannerError> {
    let mut works = Vec::new();
    let mut page = 1;
    loop {
        let batch = db
            .list_works(
                &WorkFilter::default(),
                &Pagination {
                    page,
                    per_page: PAGE_SIZE,
                },
            )
            .await?;
        let fetched = batch.items.len();
        works.extend(batch.items);
        if fetched < usize::try_from(PAGE_SIZE).unwrap_or(usize::MAX) {
            break;
        }
        page += 1;
    }

    let normalised: Vec<String> = works.iter().map(|w| normalise_title(&w.title)).collect();
    let mut pairs = Vec::new();
    for i in 0..works.len() {
        for j in (i + 1)..works.len() {
            let similarity = similarity_of_normalised(&normalised[i], &normalised[j]);
            if similarity >= threshold {
                pairs.push(NearDuplicate {
                    work_a: works[i].clone(), // clone: a work may appear in several pairs
                    work_b: works[j].clone(), // clone: a work may appear in several pairs
                    similarity,
                });
            }
        }
    }
    pairs.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
    Ok(pairs)
}

/// Lowercase, drop punctuation, collapse whitespace, strip a leading English article.
pub fn normalise_title(title: &str) -> String {
    let lowered: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = collapsed.strip_prefix(article) {
            return rest.to_string();
        }
    }
    collapsed
}

/// Similarity in 0.0..=1.0 between two raw titles, after normalisation.
pub fn title_similarity(a: &str, b: &str) -> f64 {
    similarity_of_normalised(&normalise_title(a), &normalise_title(b))
}

fn similarity_of_normalised(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    strsim::normalized_levenshtein(a, b)
}

pub fn are_near_duplicates(a: &str, b: &str, threshold: f64) -> bool {
    title_similarity(a, b) >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case("The Left Hand of Darkness", "left hand of darkness")]
    #[case("Dune: Messiah!", "dune messiah")]
    #[case("  A   Wizard of   Earthsea ", "wizard of earthsea")]
    #[case("An Unkindness", "unkindness")]
    #[case("Theory", "theory")]
    fn normalise_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(normalise_title(input), expected);
    }

    #[rstest]
    #[case("The Left Hand of Darkness", "Left Hand of Darkness", true)]
    #[case("Dune Messiah", "Dune: Messiah", true)]
    #[case("Children of Dune", "Children of Dune (Deluxe)", false)]
    #[case("Dune", "Neuromancer", false)]
    #[case("Foundation and Empire", "Foundation and Earth", false)]
    #[case("Foundation", "Foundation and Empire", false)]
    fn near_duplicate_cases(#[case] a: &str, #[case] b: &str, #[case] expected: bool) {
        assert_eq!(are_near_duplicates(a, b, DEFAULT_NEAR_THRESHOLD), expected);
    }

    fn mutate(title: &str, edits: &[(usize, char)]) -> String {
        let mut chars: Vec<char> = title.chars().collect();
        for (pos, c) in edits {
            let idx = 2 + pos % (chars.len() - 2);
            chars[idx] = *c;
        }
        chars.into_iter().collect()
    }

    proptest! {
        #[test]
        fn titles_within_two_edits_are_detected(
            title in "[b-z](?: ?[a-z]){19,39}",
            edits in proptest::collection::vec((any::<usize>(), proptest::char::range('b', 'z')), 0..=2),
        ) {
            let mutated = mutate(&title, &edits);
            prop_assert!(
                are_near_duplicates(&title, &mutated, DEFAULT_NEAR_THRESHOLD),
                "{title:?} vs {mutated:?} = {}",
                title_similarity(&title, &mutated)
            );
        }

        #[test]
        fn similarity_is_symmetric_and_bounded(a in "[a-z ]{0,30}", b in "[a-z ]{0,30}") {
            let ab = title_similarity(&a, &b);
            let ba = title_similarity(&b, &a);
            prop_assert!((0.0..=1.0).contains(&ab));
            prop_assert!((ab - ba).abs() < f64::EPSILON);
        }

        #[test]
        fn identical_titles_are_fully_similar(a in "[a-zA-Z ]{1,30}") {
            prop_assert!((title_similarity(&a, &a) - 1.0).abs() < f64::EPSILON);
        }
    }
}
