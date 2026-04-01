mod cli;

use clap::Parser;
use cli::{Cli, Command};
use curatarr_api::router::build_router;
use curatarr_api::state::AppState;
use curatarr_config::AppConfig;
use curatarr_db::create_repository;
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
        Command::Scan { path } => {
            tracing::info!("Scan not yet implemented: {}", path.display());
        }
        Command::Import { path } => {
            tracing::info!("Import not yet implemented: {}", path.display());
        }
    }

    Ok(())
}

async fn serve(
    config: AppConfig,
    port_override: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = create_repository(&config.database.url).await?;
    let state = AppState { db };
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

async fn migrate(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let _ = create_repository(&config.database.url).await?;
    tracing::info!("Migrations applied successfully");
    Ok(())
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
