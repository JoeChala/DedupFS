use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DEDUPFS_DIR: &str = ".dedupfs";
const OBJECTS_DIR: &str = "objects";
const METADATA_DIR: &str = "metadata";
const REPOSITORY_MARKER: &str = "repository";

pub struct Repository {
    root: PathBuf, //struct needs its own path
}

impl Repository {
    pub fn init(root: &Path) -> io::Result<Self> {
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
    /*
    pub fn objects_path(&self) -> PathBuf {
        self.root.join(DEDUPFS_DIR).join(OBJECTS_DIR)
    }*/

    pub fn metadata_path(&self) -> PathBuf {
        self.root.join(DEDUPFS_DIR).join(METADATA_DIR)
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
    fn initialization_is_idempotent() {
        let directory = temporary_directory();

        let first = Repository::init(&directory);
        assert!(first.is_ok());

        let second = Repository::init(&directory);
        assert!(second.is_ok());

        assert!(Repository::is_repository(&directory));

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }
}
