use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use dedupfs::{cas, dedup, gc, metadata, repository};

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

        #[arg(short, long, default_value_t = 4)]
        workers: usize,
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
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    Stats,
}
#[derive(Subcommand)]
#[command(name = "snapshot")]
enum SnapshotCommand {
    Create {
        name: String,
    },

    List,

    Delete {
        name: String,
    },

    Restore {
        name: String,
        path: PathBuf,
        destination: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

        Commands::Ingest { path, workers } => {
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

            let ingestion_result = if workers == 1 {
                engine.ingest(&path)
            } else {
                engine.ingest_parallel(&path, workers)
            };

            match ingestion_result {
                Ok(manifest) => {
                    let total_chunks = manifest.chunks().len();

                    println!(
                        "Ingested {} into {} chunks using {} worker(s).",
                        path.display(),
                        total_chunks,
                        workers
                    );
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
                Ok(stats) => {
                    println!(
                        "Garbage collection removed {} objects.",
                        stats.objects_removed
                    );
                    println!("Reclaimed {} bytes.", stats.bytes_reclaimed);
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
        Commands::Snapshot { command } => {
            let repository = repository::Repository::open(Path::new("."))?;

            let metadata = metadata::MetadataStore::open(&repository.metadata_database_path())?;

            match command {
                SnapshotCommand::Create { name } => {
                    metadata.create_snapshot(&name)?;
                    println!("Snapshot '{name}' created.");
                }

                SnapshotCommand::List => {
                    let snapshots = metadata.list_snapshots()?;

                    for snapshot in snapshots {
                        println!("{}  {}", snapshot.id, snapshot.name);
                    }
                }

                SnapshotCommand::Delete { name } => {
                    metadata.delete_snapshot(&name)?;
                    println!("Snapshot '{name}' deleted.");
                }

                SnapshotCommand::Restore {
                    name,
                    path,
                    destination,
                } => {
                    let cas = cas::Cas::new(&repository);

                    let engine = dedup::DedupEngine::new(&cas, &metadata);

                    engine.restore_snapshot(&name, &path, &destination)?;

                    println!(
                        "Restored '{}' from snapshot '{}' to '{}'.",
                        path.display(),
                        name,
                        destination.display()
                    );
                }
            }
        }

        Commands::Stats => {
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

            match engine.stats() {
                Ok(stats) => {
                    println!("Repository statistics");
                    println!("---------------------");
                    println!("Files:                {}", stats.file_count);
                    println!("Logical size:         {} bytes", stats.logical_size);
                    println!("Deduplicated size:    {} bytes", stats.deduplicated_size);
                    println!("Physical CAS size:    {} bytes", stats.physical_size);
                    println!("Unique chunks:        {}", stats.unique_chunk_count);

                    let savings = if stats.logical_size == 0 {
                        0.0
                    } else {
                        100.0 * (1.0 - stats.deduplicated_size as f64 / stats.logical_size as f64)
                    };

                    println!("Storage savings: {savings:.2}%");
                }
                Err(error) => {
                    eprintln!("Failed to calculate repository statistics: {error}");
                    std::process::exit(1);
                }
            }
        }
    }
    Ok(())
}
