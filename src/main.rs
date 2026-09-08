use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod cas;
mod chunker;
mod dedup;
mod file_reader;
mod gc;
mod hasher;
mod metadata;
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
    // Restore file from saved chunks
    Restore {
        path: PathBuf,
        destination: PathBuf,
    },
    // Garbage Collector
    Gc,
    // Remove duplicate files
    Remove {
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
            let metadata = metadata::MetadataStore::open(&repository.metadata_database_path())
                .expect("failed to open metadata database");

            metadata
                .initialize()
                .expect("failed to initialize metadata database");

            println!(
                "Initialized DedupFS repository at {}.",
                current_directory.join(".dedupfs").display()
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

            let metadata = match metadata::MetadataStore::open(&repository.metadata_database_path())
            {
                Ok(metadata) => metadata,
                Err(error) => {
                    eprintln!("Failed to open metadata database: {error}");
                    std::process::exit(1);
                }
            };

            let cas = cas::Cas::new(&repository);
            let engine = dedup::DedupEngine::new(&cas, &metadata);

            match engine.ingest(&path) {
                Ok(manifest) => {
                    let total_chunks = manifest.chunks().len();

                    if let Err(error) =
                        metadata.store_or_replace_file_manifest(&path, manifest.chunks())
                    {
                        eprintln!("Failed to store file metadata: {error}");
                        std::process::exit(1);
                    }

                    println!("Ingested {} into {} chunks.", path.display(), total_chunks);
                }
                Err(error) => {
                    eprintln!("Failed to ingest {}: {error}", path.display());
                    std::process::exit(1);
                }
            }
        }

        Commands::Restore { path, destination } => {
            let current_directory = PathBuf::from(".");

            let repository = match repository::Repository::open(&current_directory) {
                Ok(repository) => repository,
                Err(error) => {
                    eprintln!("Failed to open DedupFS repository: {error}");
                    std::process::exit(1);
                }
            };

            let metadata = match metadata::MetadataStore::open(&repository.metadata_database_path())
            {
                Ok(metadata) => metadata,
                Err(error) => {
                    eprintln!("Failed to open metadata database: {error}");
                    std::process::exit(1);
                }
            };

            let cas = cas::Cas::new(&repository);
            let engine = dedup::DedupEngine::new(&cas, &metadata);

            match engine.restore(&path, &destination) {
                Ok(()) => {
                    println!("Restored {} to {}.", path.display(), destination.display());
                }
                Err(error) => {
                    eprintln!("Failed to restore {}: {error}", path.display());
                    std::process::exit(1);
                }
            }
        }

        Commands::Gc => {
            let current_directory = PathBuf::from(".");

            let repository = match repository::Repository::open(&current_directory) {
                Ok(repository) => repository,
                Err(error) => {
                    eprintln!("Failed to open DedupFS repository: {error}");
                    std::process::exit(1);
                }
            };

            let metadata = match metadata::MetadataStore::open(&repository.metadata_database_path())
            {
                Ok(metadata) => metadata,
                Err(error) => {
                    eprintln!("Failed to open metadata database: {error}");
                    std::process::exit(1);
                }
            };

            let cas = cas::Cas::new(&repository);
            let collector = gc::GarbageCollector::new(&metadata, &cas);

            match collector.collect() {
                Ok(removed) => {
                    println!("Garbage collection removed {removed} objects.");
                }
                Err(error) => {
                    eprintln!("Garbage collection failed: {error}");
                    std::process::exit(1);
                }
            }
        }

        Commands::Remove { path } => {
            let current_directory = PathBuf::from(".");

            let repository = match repository::Repository::open(&current_directory) {
                Ok(repository) => repository,
                Err(error) => {
                    eprintln!("Failed to open DedupFS repository: {error}");
                    std::process::exit(1);
                }
            };

            let metadata = match metadata::MetadataStore::open(&repository.metadata_database_path())
            {
                Ok(metadata) => metadata,
                Err(error) => {
                    eprintln!("Failed to open metadata database: {error}");
                    std::process::exit(1);
                }
            };

            match metadata.remove_file_manifest(&path) {
                Ok(()) => {
                    println!("Removed {} from DedupFS.", path.display());
                }
                Err(error) => {
                    eprintln!("Failed to remove {}: {error}", path.display());
                    std::process::exit(1);
                }
            }
        }
    }
}
