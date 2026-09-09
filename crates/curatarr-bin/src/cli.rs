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
    /// Catalogue the books/comics/manga already in a directory, leaving files where they are
    Scan {
        /// Directory to scan (registered as a root folder if it is not one yet)
        path: PathBuf,
    },
    /// Import files from a directory into the library, renaming them with the naming template
    Import {
        /// Directory to import from
        path: PathBuf,
        /// Library root folder to import into (defaults to the first configured root folder)
        #[arg(long)]
        into: Option<PathBuf>,
    },
}
