mod cli;

use clap::Parser;
use cli::{Cli, Command};
use curatarr_api::router::build_router;
use curatarr_api::state::AppState;
use curatarr_config::AppConfig;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::root_folder::NewRootFolder;
use curatarr_core::types::work::WorkFilter;
use curatarr_db::create_repository;
use curatarr_metadata::{EnrichmentService, ReqwestHttp, registry_from_config};
use curatarr_scanner::import::{ImportReport, import_directory};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

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

    let http: Arc<dyn curatarr_metadata::HttpClient> = Arc::new(ReqwestHttp::new()?);
    let registry = registry_from_config(&config.metadata, Arc::clone(&http)); // clone: providers share the client
    let metadata = EnrichmentService::new(
        Arc::clone(&db), // clone: service and HTTP handlers share the repository
        registry,
        config.metadata.match_threshold,
        Some(config.library.covers_dir()),
        http,
    );
    let interval_hours = config.metadata.refresh_interval_hours;
    let state = AppState::new(db, config.library.clone()).with_metadata(metadata); // clone: library config is also stored on AppState
    if interval_hours > 0 {
        spawn_scheduled_refresh(state.metadata.clone(), interval_hours); // clone: background task holds the service
    }
    let router = build_router(state);

    let port = port_override.unwrap_or(config.server.port);
    let addr = format!("{}:{}", config.server.host, port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("curatarr listening on {addr}");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
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
