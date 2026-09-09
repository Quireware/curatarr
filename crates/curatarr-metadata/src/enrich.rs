use crate::apply;
use crate::http::{HttpClient, MockHttp};
use crate::merge::merge;
use crate::registry::{ProviderRegistry, TrackedProvider};
use crate::score::score_match;
use chrono::Datelike;
use curatarr_core::error::{DbError, ProviderError};
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::Pagination;
use curatarr_core::types::edition::EditionFilter;
use curatarr_core::types::id::{AuthorId, SeriesId, WorkId};
use curatarr_core::types::identifiers::ExternalId;
use curatarr_core::types::metadata::{
    EntityKind, MetadataMatch, MetadataQuery, MetadataSnapshot, RefreshReport, WorkMetadata, fields,
};
use curatarr_core::types::work::{Work, WorkFilter};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum EnrichError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("work {0} not found")]
    WorkNotFound(WorkId),
    #[error("author {0} not found")]
    AuthorNotFound(AuthorId),
    #[error("series {0} not found")]
    SeriesNotFound(SeriesId),
    #[error("unknown field {0}")]
    UnknownField(String),
}

pub struct EnrichmentService {
    db: Arc<dyn Repository>,
    registry: ProviderRegistry,
    match_threshold: f64,
    covers_dir: Option<PathBuf>,
    http: Arc<dyn HttpClient>,
}

impl EnrichmentService {
    pub fn new(
        db: Arc<dyn Repository>,
        registry: ProviderRegistry,
        match_threshold: f64,
        covers_dir: Option<PathBuf>,
        http: Arc<dyn HttpClient>,
    ) -> Self {
        Self {
            db,
            registry,
            match_threshold,
            covers_dir,
            http,
        }
    }

    pub fn empty(db: Arc<dyn Repository>) -> Self {
        Self::new(
            db,
            ProviderRegistry::empty(),
            0.6,
            None,
            Arc::new(MockHttp::default()),
        )
    }

    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub async fn search(&self, query: &MetadataQuery) -> Vec<MetadataMatch> {
        let mut matches = Vec::new();
        let providers = match query.content_type {
            Some(ct) => self.registry.enabled_for(ct),
            None => self.registry.enabled().collect(),
        };
        for provider in providers {
            let started = Instant::now();
            match provider.inner.search(query).await {
                Ok(mut found) => {
                    provider.record(started, Ok(()));
                    found.retain(|m| m.score >= self.match_threshold);
                    matches.extend(found);
                }
                Err(e) => {
                    provider.record(started, Err(&e));
                    tracing::warn!(provider = provider.name(), error = %e, "metadata search failed");
                }
            }
        }
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }

    pub async fn refresh_work(
        &self,
        work_id: WorkId,
        preview: bool,
    ) -> Result<RefreshReport, EnrichError> {
        let ctx = self.load_context(work_id).await?;
        let mut matches = Vec::new();
        let mut candidates = Vec::new();
        let mut provider_errors = Vec::new();
        for provider in self.registry.enabled_for(ctx.work.content_type) {
            match self.candidate_from(&provider, &ctx).await {
                Ok(Some((m, meta))) => {
                    matches.push(m);
                    candidates.push((provider.name().to_string(), MetadataSnapshot::from(&meta)));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(provider = provider.name(), error = %e, "provider failed during refresh");
                    provider_errors.push(format!("{}: {e}", provider.name()));
                }
            }
        }
        let result = merge(&ctx.current, &candidates, &ctx.locks);
        if !preview {
            apply::persist(
                self.db.as_ref(),
                self.http.as_ref(),
                self.covers_dir.as_deref(),
                work_id,
                &result.snapshot,
                &result.applied,
                &matches,
            )
            .await?;
        }
        Ok(RefreshReport {
            work_id,
            matches,
            applied: result.applied,
            skipped_locked: result.skipped_locked,
            conflicts: result.conflicts,
            provider_errors,
            preview,
        })
    }

    pub async fn apply_match(
        &self,
        work_id: WorkId,
        provider_name: &str,
        external_id: &ExternalId,
        preview: bool,
    ) -> Result<RefreshReport, EnrichError> {
        let ctx = self.load_context(work_id).await?;
        let tracked =
            self.registry
                .get(provider_name)
                .ok_or_else(|| ProviderError::Unsupported {
                    provider: provider_name.to_string(),
                    feature: "unknown provider".into(),
                })?;
        let started = Instant::now();
        let meta = tracked
            .inner
            .fetch_work(external_id)
            .await
            .inspect_err(|e| {
                tracked.record(started, Err(e));
            })?;
        tracked.record(started, Ok(()));
        let snap = MetadataSnapshot::from(&meta);
        let result = merge(
            &ctx.current,
            &[(provider_name.to_string(), snap)],
            &ctx.locks,
        );
        let matches = vec![MetadataMatch {
            provider: provider_name.to_string(),
            external_id: external_id.clone(), // clone: report owns the selected id
            title: meta.title.clone(),        // clone: report owns match title
            authors: meta.authors.iter().map(|a| a.name.clone()).collect(),
            year: None,
            description: meta.description.clone(),
            cover_url: meta.cover_url.clone(),
            score: 1.0,
        }];
        if !preview {
            apply::persist(
                self.db.as_ref(),
                self.http.as_ref(),
                self.covers_dir.as_deref(),
                work_id,
                &result.snapshot,
                &result.applied,
                &matches,
            )
            .await?;
        }
        Ok(RefreshReport {
            work_id,
            matches,
            applied: result.applied,
            skipped_locked: result.skipped_locked,
            conflicts: result.conflicts,
            provider_errors: vec![],
            preview,
        })
    }

    pub async fn bulk_refresh(
        &self,
        filter: &WorkFilter,
        preview: bool,
    ) -> Result<Vec<RefreshReport>, EnrichError> {
        let mut reports = Vec::new();
        let mut page = 1;
        loop {
            let batch = self
                .db
                .list_works(filter, &Pagination { page, per_page: 50 })
                .await?;
            let fetched = batch.items.len();
            for work in batch.items {
                reports.push(self.refresh_work(work.id, preview).await?);
            }
            if fetched < 50 {
                break;
            }
            page += 1;
        }
        Ok(reports)
    }

    pub async fn import_author_works(
        &self,
        author_id: AuthorId,
        preview: bool,
    ) -> Result<Vec<RefreshReport>, EnrichError> {
        let author = self
            .db
            .get_author(author_id)
            .await?
            .ok_or(EnrichError::AuthorNotFound(author_id))?;
        let query = MetadataQuery {
            authors: vec![author.name.clone()], // clone: query owns the author name
            ..MetadataQuery::default()
        };
        self.import_matches(&query, Some(author_id), None, preview)
            .await
    }

    pub async fn import_series_works(
        &self,
        series_id: SeriesId,
        preview: bool,
    ) -> Result<Vec<RefreshReport>, EnrichError> {
        let series = self
            .db
            .get_series(series_id)
            .await?
            .ok_or(EnrichError::SeriesNotFound(series_id))?;
        let query = MetadataQuery {
            series: Some(series.title.clone()), // clone: query owns the series title
            title: Some(series.title.clone()),  // clone: search uses the series title
            ..MetadataQuery::default()
        };
        self.import_matches(&query, None, Some(series_id), preview)
            .await
    }

    async fn import_matches(
        &self,
        query: &MetadataQuery,
        author_id: Option<AuthorId>,
        series_id: Option<SeriesId>,
        preview: bool,
    ) -> Result<Vec<RefreshReport>, EnrichError> {
        let matches = self.search(query).await;
        let mut reports = Vec::new();
        for m in matches {
            let work_id = self.ensure_work_for_title(&m.title).await?;
            if let Some(aid) = author_id {
                match self
                    .db
                    .link_work_author(
                        work_id,
                        aid,
                        curatarr_core::types::enums::AuthorRole::Author,
                    )
                    .await
                {
                    Ok(()) | Err(DbError::Conflict(_)) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if let Some(sid) = series_id {
                let entries = self.db.list_work_series_entries(work_id).await?;
                if !entries.iter().any(|e| e.series_id == sid) {
                    self.db
                        .create_series_entry(&curatarr_core::types::series::NewSeriesEntry {
                            series_id: sid,
                            work_id,
                            position: 1.0,
                            arc: None,
                        })
                        .await?;
                }
            }
            reports.push(
                self.apply_match(work_id, &m.provider, &m.external_id, preview)
                    .await?,
            );
        }
        Ok(reports)
    }

    async fn ensure_work_for_title(&self, title: &str) -> Result<WorkId, EnrichError> {
        let page = self
            .db
            .list_works(
                &WorkFilter {
                    title_contains: Some(title.to_string()),
                    ..WorkFilter::default()
                },
                &Pagination {
                    page: 1,
                    per_page: 20,
                },
            )
            .await?;
        if let Some(existing) = page
            .items
            .into_iter()
            .find(|w| w.title.eq_ignore_ascii_case(title))
        {
            return Ok(existing.id);
        }
        let created = self
            .db
            .create_work(&curatarr_core::types::work::NewWork {
                title: title.to_string(),
                sort_title: title.to_string(),
                original_language: None,
                original_pub_date: None,
                description: None,
                description_html: None,
                content_type: curatarr_core::types::enums::ContentType::Book,
                age_rating: None,
                content_warnings: vec![],
                read_status: curatarr_core::types::enums::ReadStatus::Unread,
            })
            .await?;
        Ok(created.id)
    }

    pub async fn set_locks(&self, work_id: WorkId, fields: &[String]) -> Result<(), EnrichError> {
        self.ensure_work(work_id).await?;
        for field in fields {
            if !fields::is_known(field) {
                return Err(EnrichError::UnknownField(field.clone()));
            }
        }
        let id = work_id.to_string();
        self.db.clear_all_field_locks(EntityKind::Work, &id).await?;
        for field in fields {
            self.db.set_field_lock(EntityKind::Work, &id, field).await?;
        }
        Ok(())
    }

    async fn ensure_work(&self, work_id: WorkId) -> Result<Work, EnrichError> {
        self.db
            .get_work(work_id)
            .await?
            .ok_or(EnrichError::WorkNotFound(work_id))
    }
}

pub(crate) struct WorkContext {
    pub work: Work,
    pub current: MetadataSnapshot,
    pub locks: HashSet<String>,
}

impl EnrichmentService {
    async fn load_context(&self, work_id: WorkId) -> Result<WorkContext, EnrichError> {
        let work = self.ensure_work(work_id).await?;
        let authors = self.db.list_work_authors(work_id).await?;
        let editions = self
            .db
            .list_editions(
                &EditionFilter {
                    work_id: Some(work_id),
                    ..EditionFilter::default()
                },
                &Pagination {
                    page: 1,
                    per_page: 50,
                },
            )
            .await?;
        let series_entries = self.db.list_work_series_entries(work_id).await?;
        let series_title = match series_entries.first() {
            Some(entry) => self.db.get_series(entry.series_id).await?.map(|s| s.title),
            None => None,
        };
        let isbn13 = editions
            .items
            .iter()
            .find_map(|e| e.isbn13.as_ref().map(|i| i.as_str().to_string()));
        let page_count = editions.items.iter().find_map(|e| e.page_count);
        let current = MetadataSnapshot {
            title: Some(work.title.clone()), // clone: snapshot owns current library values
            sort_title: Some(work.sort_title.clone()), // clone: snapshot owns current library values
            original_language: work.original_language.clone(), // clone: snapshot owns current library values
            original_pub_date: work.original_pub_date,
            description: work.description.clone(), // clone: snapshot owns current library values
            age_rating: work.age_rating,
            average_rating: work.average_rating,
            authors: authors.iter().map(|a| a.name.clone()).collect(), // clone: snapshot owns current library values
            series_title,
            series_position: series_entries.first().map(|e| e.position),
            tags: vec![],
            isbn13,
            page_count,
            cover_url: None,
        };
        let locks = self
            .db
            .list_field_locks(EntityKind::Work, &work_id.to_string())
            .await?
            .into_iter()
            .map(|l| l.field)
            .collect();
        Ok(WorkContext {
            work,
            current,
            locks,
        })
    }

    async fn candidate_from(
        &self,
        provider: &TrackedProvider,
        ctx: &WorkContext,
    ) -> Result<Option<(MetadataMatch, WorkMetadata)>, EnrichError> {
        self.candidate_from_clean(provider, ctx, Instant::now())
            .await
    }

    async fn candidate_from_clean(
        &self,
        provider: &TrackedProvider,
        ctx: &WorkContext,
        started: Instant,
    ) -> Result<Option<(MetadataMatch, WorkMetadata)>, EnrichError> {
        let existing = self
            .db
            .list_external_ids(EntityKind::Work, &ctx.work.id.to_string())
            .await?;
        if let Some(id) = existing
            .iter()
            .find(|id| id.provider_name() == provider.name())
        {
            return self.fetch_candidate(provider, id, ctx, started).await;
        }
        let query = query_from(&ctx.current, ctx.work.content_type);
        match provider.inner.search(&query).await {
            Ok(found) => {
                provider.record(started, Ok(()));
                let best = found.into_iter().max_by(|a, b| {
                    a.score
                        .partial_cmp(&b.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                match best {
                    Some(m) if m.score >= self.match_threshold => {
                        self.fetch_candidate(provider, &m.external_id, ctx, Instant::now())
                            .await
                    }
                    _ => Ok(None),
                }
            }
            Err(e) => {
                provider.record(started, Err(&e));
                Err(e.into())
            }
        }
    }

    async fn fetch_candidate(
        &self,
        provider: &TrackedProvider,
        id: &ExternalId,
        ctx: &WorkContext,
        started: Instant,
    ) -> Result<Option<(MetadataMatch, WorkMetadata)>, EnrichError> {
        match provider.inner.fetch_work(id).await {
            Ok(meta) => {
                provider.record(started, Ok(()));
                let authors: Vec<String> = meta.authors.iter().map(|a| a.name.clone()).collect();
                let score = score_match(
                    &query_from(&ctx.current, ctx.work.content_type),
                    &meta.title,
                    &authors,
                    None,
                    meta.isbn13.as_deref(),
                );
                Ok(Some((match_from(&meta, score), meta)))
            }
            Err(e) => {
                provider.record(started, Err(&e));
                Err(e.into())
            }
        }
    }
}

fn query_from(
    current: &MetadataSnapshot,
    content_type: curatarr_core::types::enums::ContentType,
) -> MetadataQuery {
    MetadataQuery {
        title: current.title.clone(), // clone: query is sent to each provider independently
        authors: current.authors.clone(), // clone: query is sent to each provider independently
        isbn: current.isbn13.clone(), // clone: query is sent to each provider independently
        year: current.original_pub_date.map(|d| d.year()),
        content_type: Some(content_type),
        series: current.series_title.clone(), // clone: query is sent to each provider independently
    }
}

fn match_from(meta: &WorkMetadata, score: f64) -> MetadataMatch {
    MetadataMatch {
        provider: meta.provider.clone(), // clone: match is an owned search hit
        external_id: meta.external_id.clone(), // clone: match is an owned search hit
        title: meta.title.clone(),       // clone: match is an owned search hit
        authors: meta.authors.iter().map(|a| a.name.clone()).collect(), // clone: match is an owned search hit
        year: meta.original_pub_date.map(|d| d.year()),
        description: meta.description.clone(), // clone: match is an owned search hit
        cover_url: meta.cover_url.clone(),     // clone: Url is not Copy
        score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::MockHttp;
    use crate::registry::ProviderRegistry;
    use async_trait::async_trait;
    use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
    use curatarr_core::types::enums::ContentType;
    use curatarr_core::types::enums::ReadStatus;
    use curatarr_core::types::work::NewWork;
    use curatarr_db::create_repository;

    struct FakeProvider {
        meta: WorkMetadata,
    }

    #[async_trait]
    impl MetadataProvider for FakeProvider {
        fn name(&self) -> &str {
            "openlibrary"
        }
        fn supported_content_types(&self) -> &[ContentType] {
            &[ContentType::Book]
        }
        async fn search(
            &self,
            _query: &MetadataQuery,
        ) -> Result<Vec<MetadataMatch>, ProviderError> {
            Ok(vec![match_from(&self.meta, 1.0)])
        }
        async fn fetch_work(&self, _id: &ExternalId) -> Result<WorkMetadata, ProviderError> {
            Ok(self.meta.clone()) // clone: each fetch returns an owned document
        }
        async fn health_check(&self) -> Result<ProviderHealth, ProviderError> {
            Ok(ProviderHealth {
                available: true,
                latency_ms: Some(1),
                last_error: None,
            })
        }
    }

    fn dune_meta() -> WorkMetadata {
        WorkMetadata {
            provider: "openlibrary".into(),
            external_id: ExternalId::OpenLibraryWork("OL45883W".into()),
            title: "Dune".into(),
            sort_title: None,
            original_language: Some("en".into()),
            original_pub_date: None,
            description: Some("Desert planet.".into()),
            content_type: ContentType::Book,
            age_rating: None,
            average_rating: Some(4.5),
            authors: vec![curatarr_core::types::metadata::AuthorRef {
                name: "Frank Herbert".into(),
                external_id: None,
            }],
            series: None,
            tags: vec!["sf".into()],
            identifiers: vec![],
            isbn13: None,
            isbn10: None,
            page_count: None,
            cover_url: None,
        }
    }

    async fn service_with_fake() -> (EnrichmentService, WorkId) {
        let db = create_repository("sqlite::memory:").await.unwrap();
        let work = db
            .create_work(&NewWork {
                title: "Dune".into(),
                sort_title: "Dune".into(),
                original_language: None,
                original_pub_date: None,
                description: None,
                description_html: None,
                content_type: ContentType::Book,
                age_rating: None,
                content_warnings: vec![],
                read_status: ReadStatus::Unread,
            })
            .await
            .unwrap();
        let tracked = crate::registry::TrackedProvider {
            inner: Arc::new(FakeProvider { meta: dune_meta() }),
            priority: 1,
            enabled: true,
            successes: std::sync::atomic::AtomicU64::new(0),
            failures: std::sync::atomic::AtomicU64::new(0),
            last_latency_ms: std::sync::atomic::AtomicU64::new(0),
        };
        let registry = ProviderRegistry::from_tracked(vec![Arc::new(tracked)]);
        let svc = EnrichmentService::new(db, registry, 0.5, None, Arc::new(MockHttp::default()));
        (svc, work.id)
    }

    #[tokio::test]
    async fn refresh_fills_empty_description_and_honours_lock() {
        let (svc, id) = service_with_fake().await;
        let report = svc.refresh_work(id, false).await.unwrap();
        assert!(
            report
                .applied
                .iter()
                .any(|c| c.field == fields::DESCRIPTION),
            "expected description to be applied, got {:?}",
            report.applied
        );
        svc.set_locks(id, &[fields::DESCRIPTION.to_string()])
            .await
            .unwrap();
        let preview = svc.refresh_work(id, true).await.unwrap();
        assert!(
            preview
                .skipped_locked
                .contains(&fields::DESCRIPTION.to_string())
        );
    }
}
