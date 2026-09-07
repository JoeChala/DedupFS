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
}
