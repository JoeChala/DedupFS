use std::path::{Path, PathBuf};
use std::{fs, io};

const DEDUPFS_DIR: &str = ".dedupfs";
const OBJECTS_DIR: &str = "objects";
const METADATA_DIR: &str = "metadata";
const REPOSITORY_MARKER: &str = "repository";
const METADATA_DATABASE: &str = "dedupfs.db";

#[derive(Debug)]
pub struct Repository {
    root: PathBuf, //struct needs its own path
}

impl Repository {
    pub fn init(root: &Path) -> io::Result<Self> {
        if Self::is_repository(root) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a DedupFS repository already exists",
            ));
        }

        let dedupfs_dir = root.join(DEDUPFS_DIR);

        fs::create_dir_all(dedupfs_dir.join(OBJECTS_DIR))?;
        fs::create_dir_all(dedupfs_dir.join(METADATA_DIR))?;
        fs::File::create(dedupfs_dir.join(REPOSITORY_MARKER))?;

        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn is_repository(root: &Path) -> bool {
        root.join(DEDUPFS_DIR).join(REPOSITORY_MARKER).is_file()
    }

    pub fn objects_path(&self) -> PathBuf {
        self.root.join(DEDUPFS_DIR).join(OBJECTS_DIR)
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.root.join(DEDUPFS_DIR).join(METADATA_DIR)
    }

    pub fn metadata_database_path(&self) -> PathBuf {
        self.metadata_path().join(METADATA_DATABASE)
    }

    pub fn open(root: &Path) -> io::Result<Self> {
        if !Self::is_repository(root) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "not a DedupFS repository",
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
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

        std::env::temp_dir().join(format!("dedupfs-test-{timestamp}"))
    }

    #[test]
    fn initializes_repository_structure() {
        let directory = temporary_directory();

        let repository =
            Repository::init(&directory).expect("repository initialization should succeed");

        assert!(directory.join(".dedupfs").is_dir());
        //assert!(repository.objects_path().is_dir());
        assert!(repository.metadata_path().is_dir());
        assert!(directory.join(".dedupfs/repository").is_file());

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }
    #[test]
    fn init_rejects_existing_repository() {
        let temporary_directory = tempfile::tempdir().expect("failed to create temp directory");

        Repository::init(temporary_directory.path()).expect("first initialization should succeed");

        let error = Repository::init(temporary_directory.path())
            .expect_err("second initialization should fail");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }
    #[test]
    fn detects_initialized_repository() {
        let directory = temporary_directory();

        Repository::init(&directory).expect("repository initialization should succeed");

        assert!(Repository::is_repository(&directory));

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn rejects_non_repository_directory() {
        let directory = temporary_directory();

        fs::create_dir_all(&directory).expect("test directory should be created");

        assert!(!Repository::is_repository(&directory));

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn init_creates_repository_structure() {
        let temporary_directory = tempfile::tempdir().expect("failed to create temp directory");

        let repository = Repository::init(temporary_directory.path())
            .expect("repository initialization should succeed");

        assert!(Repository::is_repository(temporary_directory.path()));
        assert!(repository.objects_path().is_dir());
        assert!(repository.metadata_path().is_dir());
        assert!(!repository.metadata_database_path().exists());
    }
    #[test]
    fn initialization_rejects_existing_repository() {
        let temporary_directory = tempfile::tempdir().expect("failed to create temp directory");

        Repository::init(temporary_directory.path()).expect("first initialization should succeed");

        let second = Repository::init(temporary_directory.path())
            .expect_err("second initialization should fail");

        assert_eq!(second.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn opens_initialized_repository() {
        let directory = temporary_directory();

        Repository::init(&directory).expect("repository initialization should succeed");

        let repository = Repository::open(&directory).expect("initialized repository should open");

        assert_eq!(
            repository.objects_path(),
            directory.join(".dedupfs/objects")
        );

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }
}
