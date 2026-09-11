use std::fs;
use std::path::{Path, PathBuf};

use dedupfs::cas::Cas;
use dedupfs::dedup::DedupEngine;
use dedupfs::metadata::MetadataStore;
use dedupfs::repository::Repository;

fn temporary_directory(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("dedupfs-integration-{name}-{}", std::process::id()));

    if directory.exists() {
        fs::remove_dir_all(&directory).expect("temporary directory should be removable");
    }

    fs::create_dir_all(&directory).expect("temporary directory should be creatable");

    directory
}

fn create_repository(directory: &Path) -> (Repository, MetadataStore, Cas) {
    let repository = Repository::init(directory).expect("repository initialization should succeed");

    let metadata = MetadataStore::open(&repository.metadata_database_path())
        .expect("metadata store should open successfully");

    metadata
        .initialize()
        .expect("metadata schema should initialize");

    let cas = Cas::new(&repository);

    (repository, metadata, cas)
}

#[test]
fn ingest_and_restore_preserves_file_contents() {
    let directory = temporary_directory("round-trip");

    let (_repository, metadata, cas) = create_repository(&directory);

    let input_path = directory.join("input.txt");
    let restored_path = directory.join("restored.txt");

    let original_contents = b"DedupFS integration test: ingest this file and restore it exactly.";

    fs::write(&input_path, original_contents).expect("input file should be writable");

    let engine = DedupEngine::new(&cas, &metadata);

    let manifest = engine
        .ingest(&input_path)
        .expect("ingestion should succeed");

    metadata
        .store_or_replace_file_manifest(&input_path, manifest.chunks())
        .expect("manifest should be stored");

    engine
        .restore(&input_path, &restored_path)
        .expect("restore should succeed");

    let restored_contents = fs::read(&restored_path).expect("restored file should be readable");

    assert_eq!(restored_contents, original_contents);

    fs::remove_dir_all(directory).expect("temporary directory should be removable");
}
#[test]
fn restore_fails_when_cas_object_is_missing() {
    let directory = temporary_directory("missing-object");

    let (_repository, metadata, cas) = create_repository(&directory);

    let input_path = directory.join("input.txt");
    let restored_path = directory.join("restored.txt");

    fs::write(&input_path, b"data that will be stored in CAS")
        .expect("input file should be writable");

    let engine = DedupEngine::new(&cas, &metadata);

    let manifest = engine
        .ingest(&input_path)
        .expect("ingestion should succeed");

    metadata
        .store_or_replace_file_manifest(&input_path, manifest.chunks())
        .expect("manifest should be stored");

    engine
        .restore(&input_path, &restored_path)
        .expect("restore should succeed");

    assert!(!manifest.chunks().is_empty());

    let hash = &manifest.chunks()[0];

    cas.remove(hash).expect("CAS object should be removable");

    let result = engine.restore(&input_path, &restored_path);

    assert!(result.is_err());

    fs::remove_dir_all(directory).expect("temporary directory should be removable");
}
#[test]
fn restore_fails_when_cas_object_is_corrupted() {
    let directory = temporary_directory("corrupted-object");

    let (_repository, metadata, cas) = create_repository(&directory);

    let input_path = directory.join("input.txt");
    let restored_path = directory.join("restored.txt");

    fs::write(
        &input_path,
        b"this data should be protected by the content hash",
    )
    .expect("input file should be writable");

    let engine = DedupEngine::new(&cas, &metadata);

    let manifest = engine
        .ingest(&input_path)
        .expect("ingestion should succeed");

    metadata
        .store_or_replace_file_manifest(&input_path, manifest.chunks())
        .expect("manifest should be stored");

    engine
        .restore(&input_path, &restored_path)
        .expect("restore should succeed");

    let hash = &manifest.chunks()[0];

    let object_path = directory.join(".dedupfs").join("objects").join(hash);

    fs::write(&object_path, b"corrupted data").expect("CAS object should be writable");

    let result = engine.restore(&input_path, &restored_path);

    assert!(result.is_err());

    fs::remove_dir_all(directory).expect("temporary directory should be removable");
}
