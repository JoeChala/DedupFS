use crossbeam_channel as mpsc;
use std::io::{self, Read};
use std::path::Path;
use std::thread;

use crate::cas::Cas;
use crate::chunker::{Chunker, chunk_reader};
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

pub struct StorageStats {
    pub file_count: usize,
    pub logical_size: u64,
    pub deduplicated_size: u64,
    pub physical_size: u64,
    pub unique_chunk_count: usize,
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

    pub fn restore_snapshot(
        &self,
        snapshot: &str,
        path: &Path,
        destination: &Path,
    ) -> io::Result<()> {
        let chunk_hashes = self
            .metadata
            .get_snapshot_manifest(snapshot, path)
            .map_err(io::Error::other)?;

        let mut output = std::fs::File::create(destination)?;

        for hash in chunk_hashes {
            let chunk = self.cas.get(&hash)?;
            std::io::Write::write_all(&mut output, &chunk)?;
        }

        Ok(())
    }

    pub fn ingest_parallel(&self, path: &Path, worker_count: usize) -> io::Result<FileManifest> {
        if worker_count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "worker count must be greater than zero",
            ));
        }

        let mut reader = FileReader::open(path)?;
        let mut chunker = Chunker::new();

        const WORK_QUEUE_SIZE: usize = 16;

        let (work_sender, work_receiver) = mpsc::bounded::<(usize, Vec<u8>)>(WORK_QUEUE_SIZE);

        let (result_sender, result_receiver) = mpsc::unbounded::<io::Result<(usize, String)>>();

        let work_receiver = std::sync::Arc::new(std::sync::Mutex::new(work_receiver));

        let mut workers = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let receiver = std::sync::Arc::clone(&work_receiver);
            let sender = result_sender.clone();
            let cas = self.cas.clone();

            let worker = thread::spawn(move || {
                loop {
                    let work = {
                        let receiver = receiver
                            .lock()
                            .expect("work receiver mutex should not be poisoned");

                        receiver.recv()
                    };

                    let (index, chunk) = match work {
                        Ok(work) => work,
                        Err(_) => break,
                    };

                    let result = cas.put(&chunk).map(|hash| (index, hash));

                    if sender.send(result).is_err() {
                        break;
                    }
                }
            });

            workers.push(worker);
        }

        let mut buffer = [0u8; 64 * 1024];
        let mut next_index = 0;

        loop {
            let bytes_read = reader.read(&mut buffer)?;

            if bytes_read == 0 {
                break;
            }

            let mut send_error = None;

            chunker.add_bytes_with(&buffer[..bytes_read], |chunk| {
                if send_error.is_some() {
                    return;
                }

                if work_sender.send((next_index, chunk)).is_err() {
                    send_error = Some(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "worker threads stopped unexpectedly",
                    ));
                    return;
                }

                next_index += 1;
            });

            if let Some(error) = send_error {
                return Err(error);
            }
        }

        if let Some(chunk) = chunker.finish() {
            work_sender.send((next_index, chunk)).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "worker threads stopped unexpectedly",
                )
            })?;
        }

        drop(work_sender);
        drop(result_sender);

        let mut results = Vec::with_capacity(next_index);

        for result in result_receiver {
            results.push(result?);
        }

        for worker in workers {
            worker
                .join()
                .map_err(|_| io::Error::other("worker thread panicked"))?;
        }

        results.sort_unstable_by_key(|(index, _)| *index);

        let chunk_hashes = results.into_iter().map(|(_, hash)| hash).collect();

        Ok(FileManifest {
            chunks: chunk_hashes,
        })
    }

    pub fn stats(&self) -> io::Result<StorageStats> {
        let files = self.metadata.list_files().map_err(io::Error::other)?;

        let chunk_hashes = self
            .metadata
            .current_file_chunks()
            .map_err(io::Error::other)?;

        let mut logical_size = 0;
        let mut deduplicated_size = 0;
        let mut unique_chunks = std::collections::HashSet::new();

        for hash in chunk_hashes {
            let chunk_size = self.cas.size(&hash)?;

            logical_size += chunk_size;

            if unique_chunks.insert(hash) {
                deduplicated_size += chunk_size;
            }
        }

        let physical_size = self.cas.stored_size()?;

        Ok(StorageStats {
            file_count: files.len(),
            logical_size,
            deduplicated_size,
            physical_size,
            unique_chunk_count: unique_chunks.len(),
        })
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

    #[test]
    fn parallel_ingestion_preserves_chunk_order() {
        let temp_directory = temporary_directory();
        std::fs::create_dir_all(&temp_directory).unwrap();

        let repository = crate::repository::Repository::init(&temp_directory).unwrap();

        let metadata =
            crate::metadata::MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = crate::cas::Cas::new(&repository);

        let engine = DedupEngine::new(&cas, &metadata);

        let input_path = temp_directory.join("input.txt");

        let data = (0..5000)
            .map(|value| (value % 256) as u8)
            .collect::<Vec<u8>>();

        std::fs::write(&input_path, &data).unwrap();

        let sequential_manifest = engine.ingest(&input_path).unwrap();

        let parallel_manifest = engine.ingest_parallel(&input_path, 4).unwrap();

        assert_eq!(sequential_manifest.chunks(), parallel_manifest.chunks());

        std::fs::remove_dir_all(&temp_directory).unwrap();
    }

    #[test]
    fn parallel_ingestion_rejects_zero_workers() {
        let temp_directory = temporary_directory();
        std::fs::create_dir_all(&temp_directory).unwrap();

        let repository = crate::repository::Repository::init(&temp_directory).unwrap();

        let metadata =
            crate::metadata::MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = crate::cas::Cas::new(&repository);

        let engine = DedupEngine::new(&cas, &metadata);

        let input_path = temp_directory.join("input.txt");

        std::fs::write(&input_path, b"hello").unwrap();

        let result = engine.ingest_parallel(&input_path, 0);

        assert!(result.is_err());

        std::fs::remove_dir_all(&temp_directory).unwrap();
    }
}
