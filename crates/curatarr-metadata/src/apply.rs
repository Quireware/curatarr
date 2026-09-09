use crate::enrich::EnrichError;
use crate::http::{HttpClient, HttpMethod, HttpRequest, check_status};
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::author::NewAuthor;
use curatarr_core::types::edition::{EditionFilter, EditionUpdate};
use curatarr_core::types::enums::{AuthorRole, ReadingOrder, SeriesType};
use curatarr_core::types::id::WorkId;
use curatarr_core::types::metadata::{
    EntityKind, FieldChange, MetadataMatch, MetadataSnapshot, NewAuditEvent, fields,
};
use curatarr_core::types::series::{NewSeries, NewSeriesEntry};
use curatarr_core::types::tag::NewTag;
use curatarr_core::types::work::WorkUpdate;
use curatarr_scanner::cover::process_cover;
use curatarr_scanner::naming::sort_name_for;
use std::path::Path;

pub(crate) async fn persist(
    db: &dyn Repository,
    http: &dyn HttpClient,
    covers_dir: Option<&Path>,
    work_id: WorkId,
    snapshot: &MetadataSnapshot,
    applied: &[FieldChange],
    matches: &[MetadataMatch],
) -> Result<(), EnrichError> {
    if !applied.is_empty() {
        db.update_work(work_id, &work_update(snapshot, applied))
            .await?;
    }
    write_audit(db, work_id, applied).await?;
    write_sources(db, work_id, applied).await?;
    write_external_ids(db, work_id, matches).await?;
    if applied.iter().any(|c| c.field == fields::AUTHORS) {
        sync_authors(db, work_id, &snapshot.authors).await?;
    }
    if applied.iter().any(|c| c.field == fields::SERIES) {
        if let Some(title) = snapshot.series_title.as_deref() {
            sync_series(db, work_id, title, snapshot.series_position).await?;
        }
    }
    if applied.iter().any(|c| c.field == fields::TAGS) {
        sync_tags(db, work_id, &snapshot.tags).await?;
    }
    if applied
        .iter()
        .any(|c| c.field == fields::ISBN13 || c.field == fields::PAGE_COUNT)
    {
        update_edition(db, work_id, snapshot).await?;
    }
    if applied.iter().any(|c| c.field == fields::COVER) {
        if let (Some(url), Some(dir)) = (snapshot.cover_url.as_ref(), covers_dir) {
            if let Err(e) = fetch_cover(db, http, work_id, url, dir).await {
                tracing::warn!(error = %e, "cover download failed");
            }
        }
    }
    Ok(())
}

fn work_update(snapshot: &MetadataSnapshot, applied: &[FieldChange]) -> WorkUpdate {
    let mut update = WorkUpdate::default();
    for change in applied {
        match change.field.as_str() {
            fields::TITLE => update.title = snapshot.title.clone(), // clone: WorkUpdate stores owned strings
            fields::SORT_TITLE => update.sort_title = snapshot.sort_title.clone(), // clone: WorkUpdate stores owned strings
            fields::ORIGINAL_LANGUAGE => {
                update.original_language = Some(snapshot.original_language.clone()) // clone: WorkUpdate stores owned strings
            }
            fields::ORIGINAL_PUB_DATE => {
                update.original_pub_date = Some(snapshot.original_pub_date)
            }
            fields::DESCRIPTION => update.description = Some(snapshot.description.clone()), // clone: WorkUpdate stores owned strings
            fields::AGE_RATING => update.age_rating = Some(snapshot.age_rating),
            fields::AVERAGE_RATING => update.average_rating = Some(snapshot.average_rating),
            _ => {}
        }
    }
    update
}

async fn write_audit(
    db: &dyn Repository,
    work_id: WorkId,
    applied: &[FieldChange],
) -> Result<(), EnrichError> {
    let id = work_id.to_string();
    for change in applied {
        db.append_audit(&NewAuditEvent {
            entity_kind: EntityKind::Work,
            entity_id: id.clone(), // clone: each audit row stores the work id
            field: change.field.clone(),
            old_value: change.old_value.clone(),
            new_value: change.new_value.clone(),
            source: change.source.clone(),
        })
        .await?;
    }
    Ok(())
}

async fn write_sources(
    db: &dyn Repository,
    work_id: WorkId,
    applied: &[FieldChange],
) -> Result<(), EnrichError> {
    let id = work_id.to_string();
    for change in applied {
        db.upsert_field_source(EntityKind::Work, &id, &change.field, &change.source)
            .await?;
    }
    Ok(())
}

async fn write_external_ids(
    db: &dyn Repository,
    work_id: WorkId,
    matches: &[MetadataMatch],
) -> Result<(), EnrichError> {
    let id = work_id.to_string();
    for m in matches {
        db.upsert_external_id(EntityKind::Work, &id, &m.external_id)
            .await?;
    }
    Ok(())
}

async fn sync_authors(
    db: &dyn Repository,
    work_id: WorkId,
    names: &[String],
) -> Result<(), EnrichError> {
    let existing = db.list_work_authors(work_id).await?;
    for name in names {
        if existing.iter().any(|a| a.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        let author = match db.find_author_by_name(name).await? {
            Some(a) => a,
            None => {
                db.create_author(&NewAuthor {
                    name: name.clone(),
                    sort_name: sort_name_for(name),
                    birth_date: None,
                    death_date: None,
                    nationality: None,
                    biography: None,
                    biography_html: None,
                    external_ids: vec![],
                })
                .await?
            }
        };
        db.link_work_author(work_id, author.id, AuthorRole::Author)
            .await?;
    }
    Ok(())
}

async fn sync_series(
    db: &dyn Repository,
    work_id: WorkId,
    title: &str,
    position: Option<f64>,
) -> Result<(), EnrichError> {
    let series = match db.find_series_by_title(title).await? {
        Some(s) => s,
        None => {
            db.create_series(&NewSeries {
                title: title.to_string(),
                sort_title: title.to_string(),
                description: None,
                series_type: SeriesType::Ongoing,
                reading_order: ReadingOrder::Publication,
                volume_count: None,
                expected_volume_count: None,
                external_ids: vec![],
            })
            .await?
        }
    };
    let entries = db.list_work_series_entries(work_id).await?;
    if entries.iter().any(|e| e.series_id == series.id) {
        return Ok(());
    }
    db.create_series_entry(&NewSeriesEntry {
        series_id: series.id,
        work_id,
        position: position.unwrap_or(1.0),
        arc: None,
    })
    .await?;
    Ok(())
}

async fn sync_tags(
    db: &dyn Repository,
    work_id: WorkId,
    tags: &[String],
) -> Result<(), EnrichError> {
    for name in tags {
        let tag = match db.find_tag_by_name(name).await? {
            Some(t) => t,
            None => {
                db.create_tag(&NewTag {
                    name: name.clone(),
                    parent_id: None,
                })
                .await?
            }
        };
        db.tag_work(work_id, tag.id).await?;
    }
    Ok(())
}

async fn update_edition(
    db: &dyn Repository,
    work_id: WorkId,
    snapshot: &MetadataSnapshot,
) -> Result<(), EnrichError> {
    let editions = db
        .list_editions(
            &EditionFilter {
                work_id: Some(work_id),
                ..EditionFilter::default()
            },
            &Pagination {
                page: 1,
                per_page: 1,
            },
        )
        .await?;
    let Some(edition) = editions.items.first() else {
        return Ok(());
    };
    let mut update = EditionUpdate::default();
    if let Some(isbn) = snapshot.isbn13.as_deref() {
        if let Ok(parsed) = isbn.try_into() {
            update.isbn13 = Some(Some(parsed));
        }
    }
    if snapshot.page_count.is_some() {
        update.page_count = Some(snapshot.page_count);
    }
    db.update_edition(edition.id, &update).await?;
    Ok(())
}

async fn fetch_cover(
    db: &dyn Repository,
    http: &dyn HttpClient,
    work_id: WorkId,
    url: &url::Url,
    covers_dir: &Path,
) -> Result<(), EnrichError> {
    let response = http
        .send(HttpRequest {
            method: HttpMethod::Get,
            url,
            headers: &[],
            json_body: None,
        })
        .await?;
    check_status("cover", &response, url.as_str())?;
    let paths = process_cover(&response.body, covers_dir).map_err(|e| {
        curatarr_core::error::ProviderError::Request {
            provider: "cover".into(),
            reason: e.to_string(),
        }
    })?;
    let dir = paths
        .original
        .parent()
        .unwrap_or(covers_dir)
        .to_string_lossy()
        .into_owned();
    let editions = db
        .list_editions(
            &EditionFilter {
                work_id: Some(work_id),
                ..EditionFilter::default()
            },
            &Pagination {
                page: 1,
                per_page: 1,
            },
        )
        .await?;
    if let Some(edition) = editions.items.first() {
        db.update_edition(
            edition.id,
            &EditionUpdate {
                cover_path: Some(Some(dir)),
                ..EditionUpdate::default()
            },
        )
        .await?;
    }
    Ok(())
}
