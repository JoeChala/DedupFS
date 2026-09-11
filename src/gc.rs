use std::io;

use crate::cas::Cas;
use crate::metadata::MetadataStore;

pub struct GarbageCollector<'a> {
    metadata: &'a MetadataStore,
    cas: &'a Cas,
}

pub struct GarbageCollectionStats {
    pub objects_removed: usize,
    pub bytes_reclaimed: u64,
}

impl<'a> GarbageCollector<'a> {
    pub fn new(metadata: &'a MetadataStore, cas: &'a Cas) -> Self {
        Self { metadata, cas }
    }
    pub fn collect(&self) -> io::Result<GarbageCollectionStats> {
        let hashes = self
            .metadata
            .unreferenced_chunks()
            .map_err(io::Error::other)?;

        let mut objects_removed = 0;
        let mut bytes_reclaimed = 0;

        for hash in hashes {
            let size = self.cas.size(&hash)?;

            self.cas.remove(&hash)?;

            self.metadata
                .remove_chunk_record(&hash)
                .map_err(io::Error::other)?;

            objects_removed += 1;
            bytes_reclaimed += size;
        }

        Ok(GarbageCollectionStats {
            objects_removed,
            bytes_reclaimed,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    use crate::repository::Repository;
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
    fn removes_unreferenced_objects() {
        let temp_directory = temporary_directory();
        std::fs::create_dir_all(&temp_directory).unwrap();

        let repository = Repository::init(&temp_directory).unwrap();

        let metadata = MetadataStore::open(&repository.metadata_database_path()).unwrap();

        metadata.initialize().unwrap();

        let cas = Cas::new(&repository);

        let hash = cas.put(b"unused data").unwrap();

        metadata
            .store_file_manifest(
                PathBuf::from("file.txt").as_path(),
                std::slice::from_ref(&hash),
            )
            .unwrap();

        metadata
            .remove_file_manifest(PathBuf::from("file.txt").as_path())
            .unwrap();

        let collector = GarbageCollector::new(&metadata, &cas);

        let stats = collector.collect().unwrap();

        assert_eq!(stats.objects_removed, 1);
        assert_eq!(stats.bytes_reclaimed, b"unused data".len() as u64);

        std::fs::remove_dir_all(&temp_directory).unwrap();
    }
}
