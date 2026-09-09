use crate::http::HttpClient;
use crate::providers;
use crate::rate_limit::TokenBucket;
use curatarr_config::metadata::{MetadataConfig, ProviderConfig};
use curatarr_core::error::ProviderError;
use curatarr_core::traits::metadata_provider::{MetadataProvider, ProviderHealth};
use curatarr_core::types::enums::ContentType;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct TrackedProvider {
    pub inner: Arc<dyn MetadataProvider>,
    pub priority: u32,
    pub enabled: bool,
    pub successes: AtomicU64,
    pub failures: AtomicU64,
    pub last_latency_ms: AtomicU64,
}

impl TrackedProvider {
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    pub async fn health(&self) -> ProviderHealth {
        match self.inner.health_check().await {
            Ok(h) => h,
            Err(e) => ProviderHealth {
                available: false,
                latency_ms: None,
                last_error: Some(e.to_string()),
            },
        }
    }

    pub fn success_rate(&self) -> Option<f64> {
        let ok = self.successes.load(Ordering::Relaxed);
        let fail = self.failures.load(Ordering::Relaxed);
        let total = ok.saturating_add(fail);
        if total == 0 {
            None
        } else {
            Some(
                f64::from(u32::try_from(ok).unwrap_or(u32::MAX))
                    / f64::from(u32::try_from(total).unwrap_or(u32::MAX)),
            )
        }
    }

    pub fn record(&self, started: Instant, result: Result<(), &ProviderError>) {
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.last_latency_ms.store(ms, Ordering::Relaxed);
        match result {
            Ok(()) => {
                self.successes.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

pub struct ProviderRegistry {
    providers: Vec<Arc<TrackedProvider>>,
}

impl ProviderRegistry {
    pub fn empty() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn from_tracked(mut providers: Vec<Arc<TrackedProvider>>) -> Self {
        providers.sort_by_key(|p| p.priority);
        Self { providers }
    }

    pub fn from_config(config: &MetadataConfig, http: Arc<dyn HttpClient>) -> Self {
        let mut specs = config.providers.clone(); // clone: registry owns independent provider specs
        specs.sort_by_key(|p| p.priority);
        let mut providers = Vec::new();
        for spec in &specs {
            if let Some(inner) = build_provider(spec, Arc::clone(&http)) {
                // clone: each provider shares the HTTP client
                providers.push(Arc::new(TrackedProvider {
                    inner,
                    priority: spec.priority,
                    enabled: spec.enabled,
                    successes: AtomicU64::new(0),
                    failures: AtomicU64::new(0),
                    last_latency_ms: AtomicU64::new(0),
                }));
            }
        }
        Self { providers }
    }

    pub fn enabled(&self) -> impl Iterator<Item = Arc<TrackedProvider>> + '_ {
        self.providers.iter().filter(|p| p.enabled).map(Arc::clone) // clone: callers hold the tracked provider while awaiting
    }

    pub fn enabled_for(&self, content_type: ContentType) -> Vec<Arc<TrackedProvider>> {
        self.enabled()
            .filter(|p| p.inner.supported_content_types().contains(&content_type))
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<TrackedProvider>> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone) // clone: caller holds the tracked provider
    }

    pub fn all(&self) -> &[Arc<TrackedProvider>] {
        &self.providers
    }
}

fn build_provider(
    spec: &ProviderConfig,
    http: Arc<dyn HttpClient>,
) -> Option<Arc<dyn MetadataProvider>> {
    let limiter = TokenBucket::new(spec.rate_limit_per_minute);
    let key = spec.api_key.as_deref();
    match spec.name.as_str() {
        "openlibrary" => Some(Arc::new(providers::openlibrary::OpenLibrary::new(
            http, limiter,
        ))),
        "googlebooks" => Some(Arc::new(providers::google_books::GoogleBooks::new(
            http, limiter, key,
        ))),
        "comicvine" => Some(Arc::new(providers::comicvine::ComicVine::new(
            http, limiter, key,
        ))),
        "anilist" => Some(Arc::new(providers::anilist::AniList::new(http, limiter))),
        "mangadex" => Some(Arc::new(providers::mangadex::MangaDex::new(http, limiter))),
        "mangaupdates" => Some(Arc::new(providers::mangaupdates::MangaUpdates::new(
            http, limiter,
        ))),
        "hardcover" => Some(Arc::new(providers::hardcover::Hardcover::new(
            http, limiter, key,
        ))),
        "isbndb" => Some(Arc::new(providers::isbndb::IsbnDb::new(http, limiter, key))),
        "myanimelist" => Some(Arc::new(providers::myanimelist::MyAnimeList::new(
            http, limiter, key,
        ))),
        "librarything" => Some(Arc::new(providers::librarything::LibraryThing::new(
            http, limiter, key,
        ))),
        "goodreads" => Some(Arc::new(providers::goodreads::Goodreads)),
        _ => {
            tracing::warn!(name = %spec.name, "unknown metadata provider in config");
            None
        }
    }
}
