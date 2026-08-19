use std::fs;
use std::io;
use std::path::Path;

const DEDUPFS_DIR: &str = ".dedupfs";
const OBJECTS_DIR: &str = "objects";
const METADATA_DIR: &str = "metadata";
const REPOSITORY_MARKER: &str = "repository";

pub fn init(root: &Path) -> io::Result<()> {
    let dedupfs_dir = root.join(DEDUPFS_DIR);

    fs::create_dir_all(dedupfs_dir.join(OBJECTS_DIR))?;
    fs::create_dir_all(dedupfs_dir.join(METADATA_DIR))?;
    fs::File::create(dedupfs_dir.join(REPOSITORY_MARKER))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("dedupfs-test-{timestamp}"))
    }

    #[test]
    fn initializes_repository_structure() {
        let directory = temporary_directory();

        init(&directory).expect("repository initialization should succeed");

        assert!(directory.join(".dedupfs").is_dir());
        assert!(directory.join(".dedupfs/objects").is_dir());
        assert!(directory.join(".dedupfs/metadata").is_dir());
        assert!(directory.join(".dedupfs/repository").is_file());

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }

    #[test]
    fn initialization_is_idempotent() {
        let directory = temporary_directory();

        init(&directory).expect("first initialization should succeed");
        init(&directory).expect("second initialization should succeed");

        assert!(directory.join(".dedupfs/objects").is_dir());
        assert!(directory.join(".dedupfs/metadata").is_dir());

        fs::remove_dir_all(&directory).expect("test directory should be removable");
    }
}
