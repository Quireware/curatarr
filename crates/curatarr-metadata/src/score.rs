use curatarr_core::types::metadata::MetadataQuery;
use curatarr_scanner::duplicates::title_similarity;

const TITLE_WEIGHT: f64 = 0.6;
const AUTHOR_WEIGHT: f64 = 0.25;
const YEAR_WEIGHT: f64 = 0.15;

/// 0.0..=1.0. ISBN exact match short-circuits to 1.0.
pub fn score_match(
    query: &MetadataQuery,
    title: &str,
    authors: &[String],
    year: Option<i32>,
    isbn: Option<&str>,
) -> f64 {
    if isbn_hit(query.isbn.as_deref(), isbn) {
        return 1.0;
    }
    let title_score = query
        .title
        .as_deref()
        .map(|q| title_similarity(q, title))
        .unwrap_or(0.0);
    let author_score = author_overlap(&query.authors, authors);
    let year_score = match (query.year, year) {
        (Some(a), Some(b)) if a == b => 1.0,
        (Some(_), Some(_)) => 0.0,
        _ => 0.0,
    };
    clamp01(title_score * TITLE_WEIGHT + author_score * AUTHOR_WEIGHT + year_score * YEAR_WEIGHT)
}

fn isbn_hit(query: Option<&str>, candidate: Option<&str>) -> bool {
    match (query, candidate) {
        (Some(q), Some(c)) => digits_only(q) == digits_only(c) && !digits_only(q).is_empty(),
        _ => false,
    }
}

fn digits_only(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn author_overlap(query: &[String], candidate: &[String]) -> f64 {
    if query.is_empty() || candidate.is_empty() {
        return 0.0;
    }
    let mut best: f64 = 0.0;
    for q in query {
        for c in candidate {
            best = best.max(title_similarity(q, c));
        }
    }
    best
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use curatarr_core::types::metadata::MetadataQuery;

    #[test]
    fn isbn_match_is_perfect() {
        let q = MetadataQuery {
            isbn: Some("9780306406157".into()),
            ..MetadataQuery::default()
        };
        assert_eq!(
            score_match(&q, "Other", &[], None, Some("978-0-306-40615-7")),
            1.0
        );
    }

    #[test]
    fn identical_title_scores_high() {
        let q = MetadataQuery {
            title: Some("Dune".into()),
            authors: vec!["Frank Herbert".into()],
            ..MetadataQuery::default()
        };
        let score = score_match(&q, "Dune", &["Frank Herbert".into()], Some(1965), None);
        assert!(score > 0.8, "score was {score}");
    }
}
