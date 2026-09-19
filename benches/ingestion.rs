use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use criterion::{Criterion, criterion_group, criterion_main};

use dedupfs::cas::Cas;
use dedupfs::dedup::DedupEngine;
use dedupfs::metadata::MetadataStore;
use dedupfs::repository::Repository;

fn temporary_directory() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("dedupfs-benchmark-{timestamp}"))
}

fn create_input_file(directory: &Path) -> PathBuf {
    let path = directory.join("input.bin");

    let data = vec![b'a'; 16 * 1024 * 1024];

    fs::write(&path, data).expect("benchmark input should be writable");

    path
}

fn create_repository(directory: &Path) -> (Repository, MetadataStore, Cas) {
    let repository = Repository::init(directory).expect("repository should initialize");

    let metadata = MetadataStore::open(&repository.metadata_database_path())
        .expect("metadata database should open");

    metadata
        .initialize()
        .expect("metadata database should initialize");

    let cas = Cas::new(&repository);

    (repository, metadata, cas)
}

fn benchmark_ingestion(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingestion");

    group.measurement_time(Duration::from_secs(15));
    group.sample_size(50);

    for workers in [1, 2, 4, 8] {
        group.bench_function(format!("{workers}_workers"), |b| {
            b.iter(|| {
                let directory = temporary_directory();

                let (_repository, metadata, cas) = create_repository(&directory);

                let input_path = create_input_file(&directory);

                let engine = DedupEngine::new(&cas, &metadata);

                let manifest = if workers == 1 {
                    engine
                        .ingest(black_box(&input_path))
                        .expect("sequential ingestion should succeed")
                } else {
                    engine
                        .ingest_parallel(black_box(&input_path), workers)
                        .expect("parallel ingestion should succeed")
                };

                black_box(manifest);

                fs::remove_dir_all(directory).expect("benchmark repository should be removable");
            });
        });
    }

    group.finish();
}

fn benchmark_deduplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplication");

    group.measurement_time(Duration::from_secs(10));
    group.sample_size(30);

    group.bench_function("repeated_ingestion", |b| {
        b.iter_batched(
            || {
                let directory = temporary_directory();

                let (_repository, metadata, cas) = create_repository(&directory);

                let input_path = create_input_file(&directory);

                let engine = DedupEngine::new(&cas, &metadata);

                engine
                    .ingest(&input_path)
                    .expect("initial ingestion should succeed");

                (directory, input_path, metadata, cas)
            },
            |(directory, input_path, metadata, cas)| {
                let engine = DedupEngine::new(&cas, &metadata);

                let manifest = engine
                    .ingest(black_box(&input_path))
                    .expect("repeated ingestion should succeed");

                black_box(manifest);

                fs::remove_dir_all(directory).expect("benchmark repository should be removable");
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn benchmark_storage_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_efficiency");

    group.measurement_time(Duration::from_secs(10));
    group.sample_size(30);

    let workloads = [("repeated_data", 0), ("mixed_data", 1), ("unique_data", 2)];

    for (name, workload) in workloads {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let directory = temporary_directory();

                    let (_repository, metadata, cas) = create_repository(&directory);

                    let input_path = directory.join("input.bin");

                    let data = match workload {
                        // Highly repetitive: excellent for deduplication.
                        0 => vec![b'a'; 16 * 1024 * 1024],

                        // Mixed workload: repeated regions plus unique regions.
                        1 => {
                            let mut data = Vec::with_capacity(16 * 1024 * 1024);

                            for i in 0..(16 * 1024) {
                                if i % 2 == 0 {
                                    data.extend_from_slice(&vec![b'a'; 1024]);
                                } else {
                                    let chunk = (i as u64).to_le_bytes();
                                    for _ in 0..128 {
                                        data.extend_from_slice(&chunk);
                                    }
                                }
                            }

                            data
                        }

                        // Unique data: each chunk should contain different content.
                        2 => {
                            let mut data = Vec::with_capacity(16 * 1024 * 1024);

                            for i in 0..(16 * 1024) {
                                let value = (i as u64).to_le_bytes();

                                for _ in 0..1024 {
                                    data.extend_from_slice(&value);
                                }
                            }

                            data.truncate(16 * 1024 * 1024);
                            data
                        }

                        _ => unreachable!(),
                    };

                    fs::write(&input_path, data).expect("benchmark input should be writable");

                    (directory, input_path, metadata, cas)
                },
                |(directory, input_path, metadata, cas)| {
                    let engine = DedupEngine::new(&cas, &metadata);

                    engine
                        .ingest(black_box(&input_path))
                        .expect("ingestion should succeed");

                    let stats = engine.stats().expect("storage statistics should succeed");

                    black_box((
                        stats.logical_size,
                        stats.deduplicated_size,
                        stats.physical_size,
                        stats.unique_chunk_count,
                    ));

                    fs::remove_dir_all(directory)
                        .expect("benchmark repository should be removable");
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_ingestion,
    benchmark_deduplication,
    benchmark_storage_efficiency
);

criterion_main!(benches);
