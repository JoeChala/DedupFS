use std::io;

use crate::cas::Cas;
use crate::metadata::MetadataStore;

pub struct VerificationStats {
    pub chunks_verified: usize,
}

pub struct RepositoryVerifier<'a> {
    metadata: &'a MetadataStore,
    cas: &'a Cas,
}

impl<'a> RepositoryVerifier<'a> {
    pub fn new(metadata: &'a MetadataStore, cas: &'a Cas) -> Self {
        Self { metadata, cas }
    }

    pub fn verify(&self) -> io::Result<VerificationStats> {
        let hashes = self
            .metadata
            .referenced_chunk_hashes()
            .map_err(io::Error::other)?;

        for hash in &hashes {
            self.cas.verify(hash).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("Missing or corrupted chunk: {hash}: {error}"),
                )
            })?;
        }

        Ok(VerificationStats {
            chunks_verified: hashes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hasher::{Hasher, Sha256Hasher};
    use crate::repository::Repository;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn verifies_referenced_chunks() {
        let temporary_directory = tempdir().expect("failed to create temporary directory");

        let objects_path = temporary_directory.path().join("objects");
        fs::create_dir_all(&objects_path).expect("failed to create objects directory");

        let database_path = temporary_directory.path().join("metadata.db");

        let repository = Repository::init(&objects_path).expect("repository should initialize");

        let metadata = MetadataStore::open(&database_path).expect("failed to open metadata");

        metadata
            .initialize()
            .expect("failed to initialize metadata");

        let cas = Cas::new(&repository);

        let data = b"hello DedupFS";
        let hash = Sha256Hasher.hash(data);

        cas.put(data).expect("failed to store CAS object");

        metadata
            .store_or_replace_file_manifest(Path::new("file.txt"), &[hash])
            .expect("failed to store manifest");

        let verifier = RepositoryVerifier::new(&metadata, &cas);

        let stats = verifier
            .verify()
            .expect("repository verification should succeed");

        assert_eq!(stats.chunks_verified, 1);
    }
}
