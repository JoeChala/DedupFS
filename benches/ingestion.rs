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

criterion_group!(benches, benchmark_ingestion, benchmark_deduplication);

criterion_main!(benches);
