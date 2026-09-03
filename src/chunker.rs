use std::io;

const CHUNK_SIZE: usize = 1024;

pub struct Chunker {
    buffer: Vec<u8>,
}

impl Chunker {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(CHUNK_SIZE),
        }
    }

    pub fn add_bytes(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        let mut remaining = bytes;

        while !remaining.is_empty() {
            let space = CHUNK_SIZE - self.buffer.len();
            let bytes_to_copy = space.min(remaining.len());

            self.buffer.extend_from_slice(&remaining[..bytes_to_copy]);

            remaining = &remaining[bytes_to_copy..];

            if self.buffer.len() == CHUNK_SIZE {
                chunks.push(std::mem::take(&mut self.buffer));
                self.buffer = Vec::with_capacity(CHUNK_SIZE);
            }
        }

        chunks
    }

    pub fn finish(&mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new()
    }
}

pub fn chunk_reader<R: io::Read>(reader: &mut R) -> io::Result<Vec<Vec<u8>>> {
    let mut chunker = Chunker::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut chunks = Vec::new();

    loop {
        let bytes_read = reader.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        chunks.extend(chunker.add_bytes(&buffer[..bytes_read]));
    }

    if let Some(chunk) = chunker.finish() {
        chunks.push(chunk);
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn creates_single_chunk_for_small_input() {
        let mut chunker = Chunker::new();

        let chunks = chunker.add_bytes(b"hello");
        let final_chunk = chunker.finish();

        assert_eq!(chunks.len(), 0);
        assert_eq!(final_chunk, Some(b"hello".to_vec()));
    }

    #[test]
    fn creates_multiple_chunks_for_large_input() {
        let mut chunker = Chunker::new();

        let data = vec![42u8; CHUNK_SIZE * 2 + 100];

        let mut chunks = chunker.add_bytes(&data);

        if let Some(final_chunk) = chunker.finish() {
            chunks.push(final_chunk);
        }

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[1].len(), CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 100);
    }

    #[test]
    fn preserves_input_data() {
        let mut chunker = Chunker::new();

        let input = b"hello dedupfs";
        let mut chunks = chunker.add_bytes(input);

        if let Some(final_chunk) = chunker.finish() {
            chunks.push(final_chunk);
        }

        let reconstructed: Vec<u8> = chunks.into_iter().flatten().collect();

        assert_eq!(reconstructed, input);
    }

    #[test]
    fn handles_input_across_multiple_add_calls() {
        let mut chunker = Chunker::new();

        let first = vec![1u8; 600];
        let second = vec![2u8; 600];

        let mut chunks = chunker.add_bytes(&first);
        chunks.extend(chunker.add_bytes(&second));

        if let Some(final_chunk) = chunker.finish() {
            chunks.push(final_chunk);
        }

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[1].len(), 176);

        assert!(chunks[0][..600].iter().all(|byte| *byte == 1));
        assert!(chunks[0][600..].iter().all(|byte| *byte == 2));
        assert!(chunks[1].iter().all(|byte| *byte == 2));
    }

    #[test]
    fn handles_empty_input() {
        let mut chunker = Chunker::new();

        assert!(chunker.add_bytes(&[]).is_empty());
        assert!(chunker.finish().is_none());
    }

    #[test]
    fn chunks_reader_data() {
        let data = vec![7u8; CHUNK_SIZE * 2 + 50];
        let mut reader = Cursor::new(data.clone());

        let chunks = chunk_reader(&mut reader).expect("reader should succeed");

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[1].len(), CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 50);

        let reconstructed: Vec<u8> = chunks.into_iter().flatten().collect();

        assert_eq!(reconstructed, data);
    }
}
