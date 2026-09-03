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
            reader: BufReader::with_capacity(BUFFER_SIZE, file),
        })
    }
}

impl Read for FileReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer)
    }
}

pub fn ingest_file(path: &Path) -> io::Result<u64> {
    let mut reader = FileReader::open(path)?;
    let chunks = crate::chunker::chunk_reader(&mut reader)?;

    let total_bytes = chunks.iter().map(|chunk| chunk.len() as u64).sum();

    Ok(total_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn temporary_file(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("dedupfs-{timestamp}-{name}"))
    }

    #[test]
    fn returns_error_for_missing_file() {
        let path = temporary_file("missing-test");

        let result = FileReader::open(&path);

        assert!(result.is_err());
    }
}
