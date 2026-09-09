mod cli;

use clap::Parser;
use cli::{Cli, Command};
use curatarr_acquire::{AcquireService, AcquireSettings};
use curatarr_api::router::build_router;
use curatarr_api::state::AppState;
use curatarr_config::AppConfig;
use curatarr_config::acquire::AcquireConfig;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::root_folder::NewRootFolder;
use curatarr_core::types::work::WorkFilter;
use curatarr_db::create_repository;
use curatarr_downloader::{Nzbget, Qbittorrent};
use curatarr_indexer::Prowlarr;
use curatarr_metadata::{EnrichmentService, HttpClient, ReqwestHttp, registry_from_config};
use curatarr_notify::Ntfy;
use curatarr_scanner::import::{ImportReport, import_directory};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing_subscriber::EnvFilter;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let config = AppConfig::load(cli.config.as_deref()).unwrap_or_else(|e| {
        eprintln!("Failed to load config: {e}");
        AppConfig::defaults()
    });

    init_tracing(&config.log);

    match cli.command {
        Command::Serve { port } => serve(config, port).await?,
        Command::Migrate => migrate(&config).await?,
        Command::Scan { path } => scan(&config, &path).await?,
        Command::Import { path, into } => import(&config, &path, into).await?,
    }

    Ok(())
}

async fn serve(
    config: AppConfig,
    port_override: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = create_repository(&config.database.url).await?;
    std::fs::create_dir_all(config.library.covers_dir())?;
    std::fs::create_dir_all(config.library.recycle_dir())?;
    sync_root_folders(&config, db.as_ref()).await?;

    let http: Arc<dyn HttpClient> = Arc::new(ReqwestHttp::new()?);
    let registry = registry_from_config(&config.metadata, Arc::clone(&http)); // clone: providers share the client
    let metadata = EnrichmentService::new(
        Arc::clone(&db), // clone: service and HTTP handlers share the repository
        registry,
        config.metadata.match_threshold,
        Some(config.library.covers_dir()),
        Arc::clone(&http), // clone: metadata and acquire share the HTTP client
    );
    bootstrap_admin(&config.auth, db.as_ref()).await?;
    let token = config.auth.resolved_token()?;
    if token.is_empty() {
        return Err("auth.api_token (or CURATARR__AUTH__API_TOKEN) is required".into());
    }
    let interval_hours = config.metadata.refresh_interval_hours;
    let mut state = AppState::new(Arc::clone(&db), config.library.clone()) // clone: library config is also stored on AppState
        .with_metadata(metadata)
        .with_api_token(token);
    if let Some(path) = config.auth.api_token_file.clone() {
        // clone: AppState stores the path for /system/reload
        state = state.with_token_file(path);
    }
    if interval_hours > 0 {
        spawn_scheduled_refresh(state.metadata.clone(), interval_hours); // clone: background task holds the service
    }
    let acquire_loops = if config.acquire.enabled() {
        let svc = Arc::new(build_acquire(&config, Arc::clone(&db), Arc::clone(&http))?);
        if let Err(e) = svc.reconcile().await {
            tracing::warn!(error = %e, "acquire reconcile failed");
        }
        state = state.with_acquire(Arc::clone(&svc)); // clone: HTTP handlers share the service with loops
        spawn_acquire_loops(svc, &config.acquire)
    } else {
        Vec::new()
    };
    let router = build_router(state);

    let port = port_override.unwrap_or(config.server.port);
    let addr = format!("{}:{}", config.server.host, port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("curatarr listening on {addr}");

    let result = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    for handle in acquire_loops {
        handle.abort();
    }
    result?;

    Ok(())
}

fn build_acquire(
    config: &AppConfig,
    db: Arc<dyn Repository>,
    http: Arc<dyn HttpClient>,
) -> Result<AcquireService, Box<dyn std::error::Error>> {
    let prowlarr = Url::parse(&config.acquire.prowlarr_url)
        .map_err(|e| format!("acquire.prowlarr_url: {e}"))?;
    let nzbget =
        Url::parse(&config.acquire.nzbget_url).map_err(|e| format!("acquire.nzbget_url: {e}"))?;
    let dest = config.acquire.nzbget_dest_dir.clone(); // clone: NZBGet client and download_roots both own the dest
    let indexer = Arc::new(Prowlarr::new(
        Arc::clone(&http), // clone: indexer shares the HTTP client
        prowlarr,
        config.acquire.prowlarr_api_key.clone(), // clone: Prowlarr client stores the API key
    ));
    let downloader = Arc::new(Nzbget::new(
        Arc::clone(&http), // clone: downloader shares the HTTP client
        nzbget,
        config.acquire.nzbget_username.clone(), // clone: NZBGet client stores credentials
        config.acquire.nzbget_password.clone(), // clone: NZBGet client stores credentials
        dest.clone(),                           // clone: acquire download_roots also owns this path
    ));
    let library_root = config
        .library
        .root_folders
        .first()
        .map(|f| f.path.clone()) // clone: acquire owns the import root
        .unwrap_or_else(|| config.library.data_dir.clone()); // clone: fallback when no root folders are set
    let mut svc = AcquireService::new(
        db,
        indexer,
        downloader,
        AcquireSettings {
            library_root,
            covers_dir: config.library.covers_dir(),
            recycle_dir: config.library.recycle_dir(),
            naming_template: config.library.naming_template.clone(), // clone: acquire owns the template
            import_mode: config.library.import_mode,
            download_roots: vec![dest],
        },
    );
    if let Some(qcfg) = &config.download_clients.qbittorrent {
        if qcfg.enabled() {
            let qurl = Url::parse(&qcfg.url)
                .map_err(|e| format!("download_clients.qbittorrent.url: {e}"))?;
            svc = svc.with_torrent(Arc::new(Qbittorrent::new(
                Arc::clone(&http), // clone: qBittorrent shares the HTTP client
                qurl,
                qcfg.username.clone(),  // clone: client stores credentials
                qcfg.password.clone(),  // clone: client stores credentials
                qcfg.save_path.clone(), // clone: client owns the save path
            )));
        }
    }
    if !config.acquire.ntfy_topic.is_empty() && !config.acquire.ntfy_url.is_empty() {
        let base =
            Url::parse(&config.acquire.ntfy_url).map_err(|e| format!("acquire.ntfy_url: {e}"))?;
        svc = svc.with_notifier(Arc::new(Ntfy::new(
            http,
            base,
            config.acquire.ntfy_topic.clone(), // clone: notifier stores the topic
        )));
    }
    Ok(svc)
}

async fn bootstrap_admin(
    auth: &curatarr_config::auth::AuthConfig,
    db: &dyn Repository,
) -> Result<(), Box<dyn std::error::Error>> {
    if db.user_count().await? > 0 {
        return Ok(());
    }
    let Some((username, password)) = auth.load_admin_credentials()? else {
        tracing::warn!("no users and no auth.admin_credentials_file; UI login is unavailable");
        return Ok(());
    };
    let hash =
        curatarr_auth::hash_password(&password).map_err(|_| "failed to hash admin password")?;
    db.create_user(&username, &hash).await?;
    tracing::info!(username = %username, "created bootstrap admin user");
    Ok(())
}

fn spawn_acquire_loops(svc: Arc<AcquireService>, cfg: &AcquireConfig) -> Vec<JoinHandle<()>> {
    let search_secs = u64::from(cfg.search_interval_minutes.max(1)).saturating_mul(60);
    let poll_secs = u64::from(cfg.poll_interval_seconds.max(1));
    let search_svc = Arc::clone(&svc); // clone: search loop and poll loop share the service
    let search = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(search_secs));
        loop {
            interval.tick().await;
            if let Err(e) = search_svc.search_wanted().await {
                tracing::warn!(error = %e, "wanted search failed");
            }
        }
    });
    let poll = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(poll_secs));
        loop {
            interval.tick().await;
            if let Err(e) = svc.poll_queue().await {
                tracing::warn!(error = %e, "queue poll failed");
            }
        }
    });
    vec![search, poll]
}

/// Register root folders declared in the config file that the database does not know yet.
async fn sync_root_folders(
    config: &AppConfig,
    db: &dyn Repository,
) -> Result<(), Box<dyn std::error::Error>> {
    let known: Vec<String> = db
        .list_root_folders()
        .await?
        .into_iter()
        .map(|f| f.path)
        .collect();
    for folder in &config.library.root_folders {
        let path = folder.path.to_string_lossy().into_owned();
        if known.contains(&path) {
            continue;
        }
        if !folder.path.is_dir() {
            tracing::warn!(path = %folder.path.display(), "configured root folder is not accessible; registering anyway");
        }
        db.create_root_folder(&NewRootFolder {
            path,
            name: folder.name.clone(),
            content_types: folder.content_types.clone().unwrap_or_default(),
        })
        .await?;
        tracing::info!(path = %folder.path.display(), "registered root folder from config");
    }
    Ok(())
}

async fn migrate(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let _ = create_repository(&config.database.url).await?;
    tracing::info!("Migrations applied successfully");
    Ok(())
}

async fn scan(config: &AppConfig, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = absolute(path)?;
    let db = create_repository(&config.database.url).await?;
    std::fs::create_dir_all(config.library.covers_dir())?;

    let registered = db
        .list_root_folders()
        .await?
        .iter()
        .any(|f| f.path == root.to_string_lossy());
    if !registered {
        db.create_root_folder(&NewRootFolder {
            path: root.to_string_lossy().into_owned(),
            name: None,
            content_types: vec![],
        })
        .await?;
        tracing::info!(path = %root.display(), "registered as root folder");
    }

    let state = AppState::new(db.clone(), config.library.clone());
    let import_config = state.scan_config(&root);
    let report = import_directory(&root, &import_config, db.as_ref()).await?;
    print_report(&report, "Scanned");
    Ok(())
}

async fn import(
    config: &AppConfig,
    path: &Path,
    into: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = absolute(path)?;
    let target = match into {
        Some(p) => absolute(&p)?,
        None => config
            .library
            .root_folders
            .first()
            .map(|f| f.path.clone())
            .ok_or("no root folder configured; pass --into <dir> or add [[library.root_folders]] to the config")?,
    };
    if !target.is_dir() {
        return Err(format!("target root folder does not exist: {}", target.display()).into());
    }
    let db = create_repository(&config.database.url).await?;
    std::fs::create_dir_all(config.library.covers_dir())?;

    let state = AppState::new(db.clone(), config.library.clone());
    let import_config = state.import_config(&target);
    let report = import_directory(&source, &import_config, db.as_ref()).await?;
    print_report(&report, "Imported");
    Ok(())
}

fn print_report(report: &ImportReport, verb: &str) {
    for result in &report.imported {
        let authors = result
            .authors
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  + {} — {} [{}] -> {}",
            result.work.title,
            if authors.is_empty() {
                "Unknown Author"
            } else {
                &authors
            },
            result.edition.format.extension(),
            result.file.path
        );
        if let Some(w) = &result.warning {
            println!("      warning: {w}");
        }
    }
    for (path, existing) in &report.duplicates {
        println!(
            "  = duplicate: {} (already at {})",
            path.display(),
            existing.path
        );
    }
    for failure in &report.failed {
        println!("  ! failed: {}: {}", failure.path.display(), failure.error);
    }
    println!(
        "{verb} {} files: {} imported, {} duplicates, {} failed",
        report.total(),
        report.imported.len(),
        report.duplicates.len(),
        report.failed.len()
    );
}

fn absolute(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn spawn_scheduled_refresh(svc: Arc<EnrichmentService>, hours: u32) {
    tokio::spawn(async move {
        let secs = u64::from(hours).saturating_mul(3600);
        let mut interval = tokio::time::interval(Duration::from_secs(secs.max(60)));
        interval.tick().await;
        loop {
            interval.tick().await;
            match svc.bulk_refresh(&WorkFilter::default(), false).await {
                Ok(reports) => {
                    tracing::info!(count = reports.len(), "scheduled metadata refresh finished")
                }
                Err(e) => tracing::warn!(error = %e, "scheduled metadata refresh failed"),
            }
        }
    });
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("Shutting down");
}

fn init_tracing(log_config: &curatarr_config::logging::LogConfig) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_config.level));

    match log_config.format {
        curatarr_config::logging::LogFormat::Json => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .json()
                .init();
        }
        curatarr_config::logging::LogFormat::Pretty => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }
}
