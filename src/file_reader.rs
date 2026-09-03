use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

const BUFFER_SIZE: usize = 64 * 1024;

pub struct FileReader {
    reader: BufReader<File>,
}

impl FileReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;

        Ok(Self {
            reader: BufReader::with_capacity(BUFFER_SIZE, file)
        })
    }

    pub fn read_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut buffer = vec![0u8; BUFFER_SIZE];

        let bytes_read = self.reader.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(None);
        }

        buffer.truncate(bytes_read);

        Ok(Some(buffer))
    }
}

pub fn ingest_file(path: &Path) -> io::Result<u64> {
    let mut reader = FileReader::open(path)?;
    let mut total_bytes = 0;

    while let Some(chunk) = reader.read_chunk()? {
        total_bytes += chunk.len() as u64;
    }

    Ok(total_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::path::PathBuf;
    fn temporary_file(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("dedupfs-{timestamp}-{name}"))
    }

    #[test]
    fn reads_file_contents() {
        let path = temporary_file("reader-test");

        fs::write(&path, b"hello dedupfs").expect("test file should be written");

        let mut reader = FileReader::open(&path).expect("file should open");

        let data = reader
            .read_chunk()
            .expect("reading should succeed")
            .expect("file should contain data");

        assert_eq!(data, b"hello dedupfs");

        fs::remove_file(&path).expect("test file should be removable");
    }

    #[test]
    fn returns_none_at_end_of_file() {
        let path = temporary_file("empty-test");

        fs::write(&path, b"").expect("test file should be written");

        let mut reader = FileReader::open(&path).expect("file should open");

        assert!(
            reader
                .read_chunk()
                .expect("reading should succeed")
                .is_none()
        );

        fs::remove_file(&path).expect("test file should be removable");
    }

    #[test]
    fn returns_error_for_missing_file() {
        let path = temporary_file("missing-test");

        let result = FileReader::open(&path);

        assert!(result.is_err());
    }

    #[test]
    fn reads_large_file_incrementally() {
        let path = temporary_file("large-test");

        let data = vec![42u8; BUFFER_SIZE * 3 + 100];

        fs::write(&path, &data).expect("test file should be written");

        let mut reader = FileReader::open(&path).expect("file should open");

        let first = reader
            .read_chunk()
            .expect("first read should succeed")
            .expect("first chunk should exist");

        let second = reader
            .read_chunk()
            .expect("second read should succeed")
            .expect("second chunk should exist");

        let third = reader
            .read_chunk()
            .expect("third read should succeed")
            .expect("third chunk should exist");

        let fourth = reader
            .read_chunk()
            .expect("fourth read should succeed")
            .expect("fourth chunk should exist");

        assert_eq!(first.len(), BUFFER_SIZE);
        assert_eq!(second.len(), BUFFER_SIZE);
        assert_eq!(third.len(), BUFFER_SIZE);
        assert_eq!(fourth.len(), 100);

        assert!(
            reader
                .read_chunk()
                .expect("final read should succeed")
                .is_none()
        );

        fs::remove_file(&path).expect("test file should be removable");
    }
}
