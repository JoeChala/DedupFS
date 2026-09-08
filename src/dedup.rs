use std::io;
use std::path::Path;

use crate::cas::Cas;
use crate::chunker::chunk_reader;
use crate::file_reader::FileReader;
use crate::metadata::MetadataStore;

pub struct FileManifest {
    chunks: Vec<String>,
}

impl FileManifest {
    pub fn chunks(&self) -> &[String] {
        &self.chunks
    }
}

pub struct DedupEngine<'a> {
    cas: &'a Cas,
    metadata: &'a MetadataStore,
}

impl<'a> DedupEngine<'a> {
    pub fn new(cas: &'a Cas, metadata: &'a MetadataStore) -> Self {
        Self { cas, metadata }
    }

    pub fn ingest(&self, path: &std::path::Path) -> io::Result<FileManifest> {
        let mut reader = FileReader::open(path)?;
        let chunks = chunk_reader(&mut reader)?;

        let mut chunk_hashes = Vec::with_capacity(chunks.len());

        for chunk in &chunks {
            let hash = self.cas.put(chunk)?;
            chunk_hashes.push(hash);
        }

        Ok(FileManifest {
            chunks: chunk_hashes,
        })
    }
    pub fn restore(&self, path: &Path, destination: &Path) -> io::Result<()> {
        let chunk_hashes = self
            .metadata
            .get_file_manifest(path)
            .map_err(io::Error::other)?;

        let mut output = std::fs::File::create(destination)?;

        for hash in chunk_hashes {
            let chunk = self.cas.get(&hash)?;
            std::io::Write::write_all(&mut output, &chunk)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::Repository;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("dedupfs-dedup-test-{timestamp}"))
    }

    #[test]
    fn ingests_file_and_returns_chunk_hashes() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);
        let engine = DedupEngine::new(&cas, &metadata);

        let file_path = directory.join("test.txt");

        fs::write(&file_path, b"hello dedupfs").expect("test file should be written");

        let manifest = engine
            .ingest(&file_path)
            .expect("file ingestion should succeed");

        assert_eq!(manifest.chunks().len(), 1);

        let hash = &manifest.chunks()[0];

        assert!(repository.objects_path().join(hash).is_file());

        fs::remove_dir_all(&directory).expect("test repository should be removable");
    }

    #[test]
    fn identical_files_produce_identical_manifests() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);
        let engine = DedupEngine::new(&cas, &metadata);

        let first_path = directory.join("first.txt");
        let second_path = directory.join("second.txt");

        fs::write(&first_path, b"same content").expect("first file should be written");
        fs::write(&second_path, b"same content").expect("second file should be written");

        let first_manifest = engine
            .ingest(&first_path)
            .expect("first ingestion should succeed");

        let second_manifest = engine
            .ingest(&second_path)
            .expect("second ingestion should succeed");

        assert_eq!(first_manifest.chunks(), second_manifest.chunks());

        fs::remove_dir_all(&directory).expect("test repository should be removable");
    }

    #[test]
    fn repeated_content_is_stored_only_once() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);
        let engine = DedupEngine::new(&cas, &metadata);

        let first_path = directory.join("first.txt");
        let second_path = directory.join("second.txt");

        fs::write(&first_path, b"same content").expect("first file should be written");
        fs::write(&second_path, b"same content").expect("second file should be written");

        engine
            .ingest(&first_path)
            .expect("first ingestion should succeed");

        engine
            .ingest(&second_path)
            .expect("second ingestion should succeed");

        let object_count = fs::read_dir(repository.objects_path())
            .expect("objects directory should be readable")
            .count();

        assert_eq!(object_count, 1);

        fs::remove_dir_all(&directory).expect("test repository should be removable");
    }

    #[test]
    fn different_content_produces_different_hashes() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);
        let engine = DedupEngine::new(&cas, &metadata);

        let first_path = directory.join("first.txt");
        let second_path = directory.join("second.txt");

        fs::write(&first_path, b"first content").expect("first file should be written");
        fs::write(&second_path, b"second content").expect("second file should be written");

        let first_manifest = engine
            .ingest(&first_path)
            .expect("first ingestion should succeed");

        let second_manifest = engine
            .ingest(&second_path)
            .expect("second ingestion should succeed");

        assert_ne!(first_manifest.chunks(), second_manifest.chunks());

        fs::remove_dir_all(&directory).expect("test repository should be removable");
    }

    #[test]
    fn rejects_missing_file() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);
        let engine = DedupEngine::new(&cas, &metadata);

        let missing_path = directory.join("missing.txt");

        let result = engine.ingest(&missing_path);

        assert!(result.is_err());

        fs::remove_dir_all(&directory).expect("test repository should be removable");
    }
    #[test]
    fn restores_ingested_file() {
        let temp_directory = temporary_directory();
        std::fs::create_dir_all(&temp_directory).unwrap();

        let repository = Repository::init(&temp_directory).unwrap();

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);
        let engine = DedupEngine::new(&cas, &metadata);

        let original_path = temp_directory.join("original.txt");
        let restored_path = temp_directory.join("restored.txt");

        let original_data = b"hello dedupfs reconstruction";

        std::fs::write(&original_path, original_data).unwrap();

        let manifest = engine.ingest(&original_path).unwrap();

        metadata
            .store_or_replace_file_manifest(&original_path, manifest.chunks())
            .unwrap();

        engine.restore(&original_path, &restored_path).unwrap();

        let restored_data = std::fs::read(&restored_path).unwrap();

        assert_eq!(restored_data, original_data);
    }
}
