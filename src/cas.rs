//Content-Addressable Storage, turn chunk's hash into a physical storage location
use crate::hasher::{Hasher, Sha256Hasher};
use crate::repository::Repository;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
#[derive(Clone)]
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
    pub fn verify(&self, hash: &str) -> io::Result<()> {
        self.get(hash)?;
        Ok(())
    }
    pub fn put(&self, data: &[u8]) -> io::Result<String> {
        let hash = self.hasher.hash(data);
        let object_path = self.object_path(&hash);

        if object_path.exists() {
            return Ok(hash);
        }

        let temporary_id = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);

        let temporary_path = self
            .objects_path
            .join(format!(".{hash}.tmp-{temporary_id}"));

        fs::write(&temporary_path, data)?;

        match fs::rename(&temporary_path, &object_path) {
            Ok(()) => Ok(hash),
            Err(_) if object_path.exists() => {
                fs::remove_file(&temporary_path).ok();
                Ok(hash)
            }
            Err(error) => {
                fs::remove_file(&temporary_path).ok();
                Err(error)
            }
        }
    }

    pub fn get(&self, hash: &str) -> io::Result<Vec<u8>> {
        let path = self.objects_path.join(hash);

        let data = fs::read(path)?;

        let actual_hash = self.hasher.hash(&data);

        if actual_hash != hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CAS object failed integrity check",
            ));
        }

        Ok(data)
    }

    pub fn remove(&self, hash: &str) -> io::Result<()> {
        let object_path = self.object_path(hash);
        fs::remove_file(object_path)
    }

    fn object_path(&self, hash: &str) -> PathBuf {
        self.objects_path.join(hash)
    }

    pub fn size(&self, hash: &str) -> io::Result<u64> {
        let object_path = self.object_path(hash);
        Ok(fs::metadata(object_path)?.len())
    }

    pub fn stored_size(&self) -> io::Result<u64> {
        let mut total = 0;

        for entry in fs::read_dir(&self.objects_path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if metadata.is_file() {
                total += metadata.len();
            }
        }

        Ok(total)
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
    fn verify_accepts_valid_object() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");

        let repository =
            Repository::init(directory.path()).expect("repository initialization should succeed");

        let cas = Cas::new(&repository);

        let hash = cas.put(b"valid content").expect("object should be stored");

        cas.verify(&hash)
            .expect("valid object should pass verification");
    }
    #[test]
    fn verify_rejects_corrupted_object() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");

        let repository =
            Repository::init(directory.path()).expect("repository initialization should succeed");

        let cas = Cas::new(&repository);

        let hash = cas
            .put(b"original content")
            .expect("object should be stored");

        let object_path = repository.objects_path().join(&hash);

        fs::write(&object_path, b"corrupted content").expect("object should be corrupted");

        let result = cas.verify(&hash);

        assert!(result.is_err(), "corrupted object should fail verification");
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
    #[test]
    fn rejects_corrupted_object() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        let cas = Cas::new(&repository);

        let hash = cas.put(b"original data").expect("object should be stored");

        let object_path = directory.join(".dedupfs").join("objects").join(&hash);

        fs::write(&object_path, b"corrupted data").expect("CAS object should be writable");

        let result = cas.get(&hash);

        assert!(result.is_err());

        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    #[test]
    fn concurrent_puts_store_one_object() {
        use std::sync::Arc;
        use std::thread;

        let repository = temporary_repository();
        let cas = Arc::new(Cas::new(&repository));

        let data = b"concurrent duplicate content";
        let mut workers = Vec::new();

        for _ in 0..8 {
            let cas = Arc::clone(&cas);

            workers.push(thread::spawn(move || {
                cas.put(data).expect("concurrent put should succeed")
            }));
        }

        let hashes: Vec<String> = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker should not panic"))
            .collect();

        assert!(hashes.windows(2).all(|pair| pair[0] == pair[1]));

        let object_path = repository.objects_path().join(&hashes[0]);

        assert!(object_path.is_file());

        let objects: Vec<_> = fs::read_dir(repository.objects_path())
            .expect("objects directory should be readable")
            .collect();

        assert_eq!(objects.len(), 1);

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
}
