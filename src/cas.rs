//Content-Addressable Storage, turn chunk's hash into a physical storage location
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::hasher::{Hasher, Sha256Hasher};
use crate::repository::Repository;

pub struct Cas {
    objects_path: PathBuf,
    hasher: Sha256Hasher,
}

impl Cas {
    pub fn new(repository: &Repository) -> Self {
        Self {
            objects_path: repository.objects_path(),
            hasher: Sha256Hasher,
        }
    }

    pub fn put(&self, data: &[u8]) -> io::Result<String> {
        let hash = self.hasher.hash(data);
        let object_path = self.object_path(&hash);

        if object_path.exists() {
            return Ok(hash);
        }

        fs::write(&object_path, data)?;

        Ok(hash)
    }
    pub fn get(&self, hash: &str) -> io::Result<Vec<u8>> {
        let object_path = self.object_path(hash);
        fs::read(object_path)
    }

    fn object_path(&self, hash: &str) -> PathBuf {
        self.objects_path.join(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("dedupfs-cas-test-{timestamp}"))
    }

    fn temporary_repository() -> Repository {
        let directory = temporary_directory();

        Repository::init(&directory).expect("repository initialization should succeed")
    }

    #[test]
    fn stores_data() {
        let repository = temporary_repository();
        let cas = Cas::new(&repository);

        let data = b"hello dedupfs";

        let hash = cas.put(data).expect("object should be stored");

        assert!(repository.objects_path().join(hash).is_file());

        fs::remove_dir_all(
            repository
                .objects_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        )
        .expect("test repository should be removable");
    }

    #[test]
    fn stores_object_under_its_hash() {
        let repository = temporary_repository();
        let cas = Cas::new(&repository);

        let data = b"hello dedupfs";

        let hash = cas.put(data).expect("object should be stored");

        assert!(repository.objects_path().join(&hash).is_file());

        fs::remove_dir_all(
            repository
                .objects_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        )
        .expect("test repository should be removable");
    }

    #[test]
    fn does_not_duplicate_existing_object() {
        let repository = temporary_repository();
        let cas = Cas::new(&repository);

        let data = b"duplicate content";

        let first_hash = cas.put(data).expect("first object should be stored");
        let first_metadata =
            fs::metadata(repository.objects_path().join(&first_hash)).expect("object should exist");

        let second_hash = cas.put(data).expect("second put should succeed");
        let second_metadata = fs::metadata(repository.objects_path().join(&second_hash))
            .expect("object should exist");

        assert_eq!(first_hash, second_hash);
        assert_eq!(first_metadata.len(), second_metadata.len());

        fs::remove_dir_all(
            repository
                .objects_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        )
        .expect("test repository should be removable");
    }

    #[test]
    fn retrieves_stored_data() {
        let repository = temporary_repository();
        let cas = Cas::new(&repository);

        let data = b"hello dedupfs";

        let hash = cas.put(data).unwrap();
        let retrieved = cas.get(&hash).unwrap();

        assert_eq!(retrieved, data);
    }
    /*
    #[test]
    fn returns_error_for_missing_object() {
        let repository = temporary_repository();
        let cas = Cas::new(&repository);

        let result = cas.get("does-not-exist");

        assert!(result.is_err());

        fs::remove_dir_all(
            repository
                .objects_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        )
        .expect("test repository should be removable");
    }

    #[test]
    fn detects_corrupted_object() {
        let repository = temporary_repository();
        let cas = Cas::new(&repository);

        let data = b"original content";
        let hash = cas.put(data).expect("object should be stored");

        fs::write(repository.objects_path().join(&hash), b"corrupted content")
            .expect("object should be corrupted");

        let result = cas.get(&hash);

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("corrupted object should fail").kind(),
            io::ErrorKind::InvalidData
        );

        fs::remove_dir_all(
            repository
                .objects_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        )
        .expect("test repository should be removable");
    }
    */
}
