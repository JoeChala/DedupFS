use std::path::Path;

use rusqlite::{Connection, Result};

pub struct MetadataStore {
    connection: Connection,
}

impl MetadataStore {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;

        Ok(Self { connection })
    }

    pub fn initialize(&self) -> Result<()> {
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS file_chunks (
                file_id INTEGER NOT NULL,
                chunk_hash TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                PRIMARY KEY (file_id, chunk_index),
                FOREIGN KEY (file_id) REFERENCES files(id)
            );
            ",
        )?;

        Ok(())
    }

    pub fn store_file_manifest(&self, path: &Path, chunk_hashes: &[String]) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        transaction.execute(
            "INSERT INTO files (path) VALUES (?1)",
            [path.to_string_lossy().as_ref()],
        )?;

        let file_id = transaction.last_insert_rowid();

        for (chunk_index, chunk_hash) in chunk_hashes.iter().enumerate() {
            transaction.execute(
                "
                INSERT INTO file_chunks (file_id, chunk_hash, chunk_index)
                VALUES (?1, ?2, ?3)
                ",
                rusqlite::params![file_id, chunk_hash, chunk_index as i64],
            )?;
        }

        transaction.commit()?;

        Ok(())
    }

    pub fn replace_file_manifest(&self, path: &Path, chunk_hashes: &[String]) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        transaction.execute(
            "DELETE FROM file_chunks
            WHERE file_id = (
                SELECT id FROM files WHERE path = ?1
            )",
            [path.to_string_lossy().as_ref()],
        )?;

        let file_id: i64 = transaction.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        for (chunk_index, chunk_hash) in chunk_hashes.iter().enumerate() {
            transaction.execute(
                "
                INSERT INTO file_chunks (file_id, chunk_hash, chunk_index)
                VALUES (?1, ?2, ?3)
                ",
                rusqlite::params![file_id, chunk_hash, chunk_index as i64],
            )?;
        }

        transaction.commit()?;

        Ok(())
    }

    fn file_exists(&self, path: &Path) -> Result<bool> {
        let exists: i64 = self.connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM files WHERE path = ?1
            )",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        Ok(exists != 0)
    }

    pub fn store_or_replace_file_manifest(
        &self,
        path: &Path,
        chunk_hashes: &[String],
    ) -> Result<()> {
        if self.file_exists(path)? {
            self.replace_file_manifest(path, chunk_hashes)
        } else {
            self.store_file_manifest(path, chunk_hashes)
        }
    }

    pub fn get_file_manifest(&self, path: &Path) -> Result<Vec<String>> {
        let file_id: i64 = self.connection.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        let mut statement = self.connection.prepare(
            "
            SELECT chunk_hash
            FROM file_chunks
            WHERE file_id = ?1
            ORDER BY chunk_index
            ",
        )?;

        let chunks = statement
            .query_map([file_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn initializes_metadata_database() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();

        store.initialize().unwrap();
    }

    #[test]
    fn stores_file_manifest() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        let chunks = vec!["hash-one".to_string(), "hash-two".to_string()];

        store
            .store_file_manifest(Path::new("test.txt"), &chunks)
            .unwrap();

        let file_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();

        let chunk_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM file_chunks", [], |row| row.get(0))
            .unwrap();

        assert_eq!(file_count, 1);
        assert_eq!(chunk_count, 2);
    }

    #[test]
    fn replaces_existing_file_manifest() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        let original_chunks = vec!["hash-one".to_string(), "hash-two".to_string()];

        store
            .store_file_manifest(Path::new("test.txt"), &original_chunks)
            .unwrap();

        let replacement_chunks = vec!["hash-three".to_string()];

        store
            .replace_file_manifest(Path::new("test.txt"), &replacement_chunks)
            .unwrap();

        let file_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();

        let chunk_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM file_chunks", [], |row| row.get(0))
            .unwrap();

        let stored_hash: String = store
            .connection
            .query_row(
                "SELECT chunk_hash FROM file_chunks WHERE file_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(file_count, 1);
        assert_eq!(chunk_count, 1);
        assert_eq!(stored_hash, "hash-three");
    }

    #[test]
    fn retrieves_file_manifest_in_chunk_order() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        let chunks = vec![
            "hash-one".to_string(),
            "hash-two".to_string(),
            "hash-three".to_string(),
        ];

        store
            .store_file_manifest(Path::new("test.txt"), &chunks)
            .unwrap();

        let manifest = store.get_file_manifest(Path::new("test.txt")).unwrap();

        assert_eq!(manifest, chunks);
    }
}
