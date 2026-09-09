use curatarr_core::types::metadata::{
    ConflictValue, FieldChange, FieldConflict, MergeResult, MetadataSnapshot, fields,
};
use serde::Serialize;
use std::collections::HashSet;
use url::Url;

/// `candidates` must already be sorted highest-priority first.
pub fn merge(
    current: &MetadataSnapshot,
    candidates: &[(String, MetadataSnapshot)],
    locks: &HashSet<String>,
) -> MergeResult {
    let mut applied = Vec::new();
    let mut skipped_locked = Vec::new();
    let mut conflicts = Vec::new();
    let snapshot = {
        let mut out = MergeOut {
            applied: &mut applied,
            skipped: &mut skipped_locked,
            conflicts: &mut conflicts,
        };

        let title = merge_string(
            fields::TITLE,
            current.title.as_deref(),
            candidates,
            |s| s.title.as_deref(),
            locks,
            &mut out,
        );
        let sort_title = merge_string(
            fields::SORT_TITLE,
            current.sort_title.as_deref(),
            candidates,
            |s| s.sort_title.as_deref(),
            locks,
            &mut out,
        );
        let original_language = merge_string(
            fields::ORIGINAL_LANGUAGE,
            current.original_language.as_deref(),
            candidates,
            |s| s.original_language.as_deref(),
            locks,
            &mut out,
        );
        let description = merge_string(
            fields::DESCRIPTION,
            current.description.as_deref(),
            candidates,
            |s| s.description.as_deref(),
            locks,
            &mut out,
        );
        let isbn13 = merge_string(
            fields::ISBN13,
            current.isbn13.as_deref(),
            candidates,
            |s| s.isbn13.as_deref(),
            locks,
            &mut out,
        );
        let series_title = merge_string(
            fields::SERIES,
            current.series_title.as_deref(),
            candidates,
            |s| s.series_title.as_deref(),
            locks,
            &mut out,
        );

        let original_pub_date = merge_copy(
            fields::ORIGINAL_PUB_DATE,
            current.original_pub_date,
            candidates,
            |s| s.original_pub_date,
            locks,
            &mut out,
        );
        let age_rating = merge_copy(
            fields::AGE_RATING,
            current.age_rating,
            candidates,
            |s| s.age_rating,
            locks,
            &mut out,
        );
        let average_rating = merge_copy(
            fields::AVERAGE_RATING,
            current.average_rating,
            candidates,
            |s| s.average_rating,
            locks,
            &mut out,
        );
        let series_position = merge_copy(
            "series_position",
            current.series_position,
            candidates,
            |s| s.series_position,
            locks,
            &mut out,
        );
        let page_count = merge_copy(
            fields::PAGE_COUNT,
            current.page_count,
            candidates,
            |s| s.page_count,
            locks,
            &mut out,
        );
        let cover_url = merge_cover(current.cover_url.as_ref(), candidates, locks, &mut out);

        let authors = merge_list(
            fields::AUTHORS,
            &current.authors,
            candidates,
            |s| &s.authors,
            locks,
            &mut out,
        );
        let tags = merge_list(
            fields::TAGS,
            &current.tags,
            candidates,
            |s| &s.tags,
            locks,
            &mut out,
        );
        MetadataSnapshot {
            title,
            sort_title,
            original_language,
            original_pub_date,
            description,
            age_rating,
            average_rating,
            authors,
            series_title,
            series_position,
            tags,
            isbn13,
            page_count,
            cover_url,
        }
    };

    MergeResult {
        snapshot,
        applied,
        skipped_locked,
        conflicts,
    }
}

struct MergeOut<'a> {
    applied: &'a mut Vec<FieldChange>,
    skipped: &'a mut Vec<String>,
    conflicts: &'a mut Vec<FieldConflict>,
}

fn merge_string(
    field: &str,
    current: Option<&str>,
    candidates: &[(String, MetadataSnapshot)],
    get: fn(&MetadataSnapshot) -> Option<&str>,
    locks: &HashSet<String>,
    out: &mut MergeOut<'_>,
) -> Option<String> {
    if locks.contains(field) {
        out.skipped.push(field.to_string());
        return current.map(str::to_string);
    }
    let winner = candidates
        .iter()
        .find_map(|(src, snap)| get(snap).map(|v| (src.as_str(), v)));
    let Some((source, value)) = winner else {
        return current.map(str::to_string);
    };
    if current.is_some_and(|c| c == value) {
        return current.map(str::to_string);
    }
    record_conflict_str(field, current, candidates, get, out.conflicts);
    out.applied.push(FieldChange {
        field: field.to_string(),
        old_value: current.map(str::to_string),
        new_value: Some(value.to_string()),
        source: source.to_string(),
    });
    Some(value.to_string())
}

fn merge_copy<T: Copy + PartialEq + Serialize>(
    field: &str,
    current: Option<T>,
    candidates: &[(String, MetadataSnapshot)],
    get: fn(&MetadataSnapshot) -> Option<T>,
    locks: &HashSet<String>,
    out: &mut MergeOut<'_>,
) -> Option<T> {
    if locks.contains(field) {
        out.skipped.push(field.to_string());
        return current;
    }
    let winner = candidates
        .iter()
        .find_map(|(src, snap)| get(snap).map(|v| (src.as_str(), v)));
    let Some((source, value)) = winner else {
        return current;
    };
    if current == Some(value) {
        return current;
    }
    record_conflict_copy(field, current, candidates, get, out.conflicts);
    out.applied.push(FieldChange {
        field: field.to_string(),
        old_value: current.map(|c| json_plain(&c)),
        new_value: Some(json_plain(&value)),
        source: source.to_string(),
    });
    Some(value)
}

fn merge_cover(
    current: Option<&Url>,
    candidates: &[(String, MetadataSnapshot)],
    locks: &HashSet<String>,
    out: &mut MergeOut<'_>,
) -> Option<Url> {
    if locks.contains(fields::COVER) {
        out.skipped.push(fields::COVER.to_string());
        return current.cloned(); // clone: Url is not Copy
    }
    let winner = candidates
        .iter()
        .find_map(|(src, snap)| snap.cover_url.as_ref().map(|u| (src.as_str(), u)));
    let Some((source, value)) = winner else {
        return current.cloned(); // clone: Url is not Copy
    };
    if current.is_some_and(|c| c == value) {
        return current.cloned(); // clone: Url is not Copy
    }
    let values = candidates
        .iter()
        .filter_map(|(src, snap)| {
            snap.cover_url.as_ref().map(|u| ConflictValue {
                source: src.clone(), // clone: conflict list owns provider names
                value: u.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if values.len() > 1 {
        out.conflicts.push(FieldConflict {
            field: fields::COVER.to_string(),
            values,
        });
    }
    out.applied.push(FieldChange {
        field: fields::COVER.to_string(),
        old_value: current.map(Url::to_string),
        new_value: Some(value.to_string()),
        source: source.to_string(),
    });
    Some(value.clone()) // clone: snapshot owns the winning cover URL
}

fn merge_list(
    field: &str,
    current: &[String],
    candidates: &[(String, MetadataSnapshot)],
    get: fn(&MetadataSnapshot) -> &[String],
    locks: &HashSet<String>,
    out: &mut MergeOut<'_>,
) -> Vec<String> {
    if locks.contains(field) {
        out.skipped.push(field.to_string());
        return current.to_vec();
    }
    let mut merged = current.to_vec();
    let mut source = "local";
    for (src, snap) in candidates {
        for name in get(snap) {
            if !merged.iter().any(|e| e.eq_ignore_ascii_case(name)) {
                merged.push(name.clone()); // clone: merged list owns each new name
                source = src.as_str();
            }
        }
    }
    if merged != current {
        out.applied.push(FieldChange {
            field: field.to_string(),
            old_value: Some(current.join(", ")),
            new_value: Some(merged.join(", ")),
            source: source.to_string(),
        });
    }
    merged
}

fn record_conflict_str(
    field: &str,
    current: Option<&str>,
    candidates: &[(String, MetadataSnapshot)],
    get: fn(&MetadataSnapshot) -> Option<&str>,
    conflicts: &mut Vec<FieldConflict>,
) {
    let mut values = Vec::new();
    if let Some(c) = current {
        values.push(ConflictValue {
            source: "local".into(),
            value: c.to_string(),
        });
    }
    for (src, snap) in candidates {
        if let Some(v) = get(snap) {
            if !values.iter().any(|e| e.value == v) {
                values.push(ConflictValue {
                    source: src.clone(), // clone: conflict list owns provider names
                    value: v.to_string(),
                });
            }
        }
    }
    if values.len() > 1 {
        conflicts.push(FieldConflict {
            field: field.to_string(),
            values,
        });
    }
}

fn record_conflict_copy<T: Copy + PartialEq + Serialize>(
    field: &str,
    current: Option<T>,
    candidates: &[(String, MetadataSnapshot)],
    get: fn(&MetadataSnapshot) -> Option<T>,
    conflicts: &mut Vec<FieldConflict>,
) {
    let mut values = Vec::new();
    if let Some(c) = current {
        values.push(ConflictValue {
            source: "local".into(),
            value: json_plain(&c),
        });
    }
    for (src, snap) in candidates {
        if let Some(v) = get(snap) {
            let rendered = json_plain(&v);
            if !values.iter().any(|e| e.value == rendered) {
                values.push(ConflictValue {
                    source: src.clone(), // clone: conflict list owns provider names
                    value: rendered,
                });
            }
        }
    }
    if values.len() > 1 {
        conflicts.push(FieldConflict {
            field: field.to_string(),
            values,
        });
    }
}

fn json_plain<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use curatarr_core::types::enums::AgeRating;
    use proptest::prelude::*;

    fn snap_title(title: &str) -> MetadataSnapshot {
        MetadataSnapshot {
            title: Some(title.into()),
            ..MetadataSnapshot::default()
        }
    }

    #[test]
    fn empty_candidates_change_nothing() {
        let current = snap_title("Dune");
        let result = merge(&current, &[], &HashSet::new());
        assert!(result.applied.is_empty());
        assert_eq!(result.snapshot.title.as_deref(), Some("Dune"));
    }

    #[test]
    fn locked_title_is_unchanged() {
        let current = snap_title("Local");
        let incoming = snap_title("Remote");
        let locks = HashSet::from([fields::TITLE.to_string()]);
        let result = merge(&current, &[("openlibrary".into(), incoming)], &locks);
        assert_eq!(result.snapshot.title.as_deref(), Some("Local"));
        assert!(result.skipped_locked.contains(&fields::TITLE.to_string()));
        assert!(result.applied.is_empty());
    }

    #[test]
    fn higher_priority_wins_and_conflict_is_recorded() {
        let current = snap_title("Local");
        let a = snap_title("Open");
        let b = snap_title("Google");
        let result = merge(
            &current,
            &[("openlibrary".into(), a), ("googlebooks".into(), b)],
            &HashSet::new(),
        );
        assert_eq!(result.snapshot.title.as_deref(), Some("Open"));
        assert_eq!(result.applied[0].source, "openlibrary");
        assert_eq!(result.conflicts.len(), 1);
    }

    #[test]
    fn authors_union_preserves_current_order() {
        let current = MetadataSnapshot {
            authors: vec!["Herbert".into()],
            ..MetadataSnapshot::default()
        };
        let incoming = MetadataSnapshot {
            authors: vec!["Herbert".into(), "Anderson".into()],
            ..MetadataSnapshot::default()
        };
        let result = merge(
            &current,
            &[("openlibrary".into(), incoming)],
            &HashSet::new(),
        );
        assert_eq!(result.snapshot.authors, vec!["Herbert", "Anderson"]);
    }

    #[test]
    fn date_is_copied_when_current_empty() {
        let current = MetadataSnapshot::default();
        let incoming = MetadataSnapshot {
            original_pub_date: NaiveDate::from_ymd_opt(1965, 8, 1),
            ..MetadataSnapshot::default()
        };
        let result = merge(
            &current,
            &[("openlibrary".into(), incoming)],
            &HashSet::new(),
        );
        assert_eq!(
            result.snapshot.original_pub_date,
            NaiveDate::from_ymd_opt(1965, 8, 1)
        );
    }

    proptest! {
        #[test]
        fn lock_is_total(title in "[A-Za-z]{1,20}", other in "[A-Za-z]{1,20}") {
            let current = snap_title(&title);
            let incoming = snap_title(&other);
            let locks = HashSet::from([fields::TITLE.to_string()]);
            let result = merge(&current, &[("p".into(), incoming)], &locks);
            prop_assert_eq!(result.snapshot.title.as_deref(), Some(title.as_str()));
        }
    }

    #[test]
    fn age_rating_serializes_in_audit() {
        let current = MetadataSnapshot::default();
        let incoming = MetadataSnapshot {
            age_rating: Some(AgeRating::Teen),
            ..MetadataSnapshot::default()
        };
        let result = merge(&current, &[("anilist".into(), incoming)], &HashSet::new());
        assert_eq!(result.applied[0].new_value.as_deref(), Some("teen"));
    }
}
