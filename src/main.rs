use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod cas;
mod chunker;
mod dedup;
mod file_reader;
mod hasher;
mod repository;

#[derive(Parser)]
#[command(name = "dedupfs")]
#[command(about = "A local content-aware storage engine")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a DedupFS repository
    Init,
    // Ingest a file
    Ingest {
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let current_directory = PathBuf::from(".");

            let repository = match repository::Repository::init(&current_directory) {
                Ok(repository) => repository,
                Err(error) => {
                    eprintln!("Failed to initialize DedupFS: {error}");
                    std::process::exit(1);
                }
            };

            println!(
                "Initialized DedupFS repository at {}.",
                repository.metadata_path().display()
            );
        }
        Commands::Ingest { path } => {
            let current_directory = PathBuf::from(".");

            let repository = match repository::Repository::open(&current_directory) {
                Ok(repository) => repository,
                Err(error) => {
                    eprintln!("Failed to open DedupFS repository: {error}");
                    std::process::exit(1);
                }
            };

            let cas = cas::Cas::new(&repository);
            let engine = dedup::DedupEngine::new(&cas);

            match engine.ingest(&path) {
                Ok(manifest) => {
                    let total_chunks = manifest.chunks().len();

                    println!("Ingested {} into {} chunks.", path.display(), total_chunks);
                }
                Err(error) => {
                    eprintln!("Failed to ingest {}: {error}", path.display());
                    std::process::exit(1);
                }
            }
        }
    }
}
