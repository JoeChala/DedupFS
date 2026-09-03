use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod chunker;
mod file_reader;
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

            if !repository::Repository::is_repository(&current_directory) {
                eprintln!("Not a DedupFS repository.");
                std::process::exit(1);
            }

            match file_reader::ingest_file(&path) {
                Ok(bytes) => {
                    println!("Read {bytes} bytes from {}.", path.display());
                }
                Err(error) => {
                    eprintln!("Failed to ingest {}: {error}", path.display());
                    std::process::exit(1);
                }
            }
        }
    }
}
