use std::fs;
use std::path::PathBuf;

use dedupfs::cas::Cas;
use dedupfs::dedup::DedupEngine;
use dedupfs::metadata::MetadataStore;
use dedupfs::repository::Repository;

fn temporary_directory() -> PathBuf {
    tempfile::tempdir()
        .expect("temporary directory should be created")
        .keep()
}

fn run_workload(name: &str, data: Vec<u8>) {
    let directory = temporary_directory();

    let repository =
        Repository::init(&directory).expect("repository initialization should succeed");

    let metadata = MetadataStore::open(&repository.metadata_database_path())
        .expect("metadata database should open");

    metadata
        .initialize()
        .expect("metadata database should initialize");

    let cas = Cas::new(&repository);

    let input_path = directory.join("input.bin");

    fs::write(&input_path, &data).expect("benchmark input should be writable");

    let engine = DedupEngine::new(&cas, &metadata);

    engine
        .ingest(&input_path)
        .expect("ingestion should succeed");

    let stats = engine.stats().expect("storage statistics should succeed");

    let savings = if stats.logical_size == 0 {
        0.0
    } else {
        (1.0 - stats.physical_size as f64 / stats.logical_size as f64) * 100.0
    };

    println!();
    println!("=== {name} ===");
    println!("Logical size:       {} bytes", stats.logical_size);
    println!("Unique chunks:      {}", stats.unique_chunk_count);
    println!("Deduplicated size:  {} bytes", stats.deduplicated_size);
    println!("Physical CAS size:  {} bytes", stats.physical_size);
    println!("Storage savings:    {savings:.2}%");

    fs::remove_dir_all(directory).expect("temporary repository should be removable");
}

#[test]
fn storage_efficiency_workloads() {
    // Highly repetitive data.
    let repeated_data = vec![b'a'; 16 * 1024 * 1024];

    run_workload("Repeated data", repeated_data);

    // Mixed workload: alternating repeated and unique chunks.
    let mut mixed_data = Vec::with_capacity(16 * 1024 * 1024);

    for i in 0..(16 * 1024) {
        if i % 2 == 0 {
            mixed_data.extend_from_slice(&vec![b'a'; 1024]);
        } else {
            let value = (i as u64).to_le_bytes();

            for _ in 0..128 {
                mixed_data.extend_from_slice(&value);
            }
        }
    }

    run_workload("Mixed data", mixed_data);

    // Mostly unique data.
    let mut unique_data = Vec::with_capacity(16 * 1024 * 1024);

    for i in 0..(16 * 1024) {
        let value = (i as u64).to_le_bytes();

        for _ in 0..1024 {
            unique_data.extend_from_slice(&value);
        }
    }

    unique_data.truncate(16 * 1024 * 1024);

    run_workload("Unique data", unique_data);
}
