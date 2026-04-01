use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "curatarr",
    version,
    about = "Ebook, comic, and manga acquisition manager"
)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start the curatarr server
    Serve {
        /// Override listen port
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Run database migrations
    Migrate,
    /// Scan a directory for books/comics/manga
    Scan {
        /// Directory path to scan
        path: PathBuf,
    },
    /// Import files from a directory into the library
    Import {
        /// Directory path to import from
        path: PathBuf,
    },
}
