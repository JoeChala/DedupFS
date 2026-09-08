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

            CREATE TABLE IF NOT EXISTS chunks (
                hash TEXT PRIMARY KEY,
                reference_count INTEGER NOT NULL
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
                INSERT INTO chunks (hash, reference_count)
                VALUES (?1, 1)
                ON CONFLICT(hash)
                DO UPDATE SET reference_count = reference_count + 1
                ",
                rusqlite::params![chunk_hash],
            )?;

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

        let file_id: i64 = transaction.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        {
            let mut statement = transaction.prepare(
                "
                SELECT chunk_hash
                FROM file_chunks
                WHERE file_id = ?1
                ",
            )?;

            let old_chunks = statement
                .query_map([file_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<String>, _>>()?;

            for chunk_hash in old_chunks {
                transaction.execute(
                    "
                    UPDATE chunks
                    SET reference_count = reference_count - 1
                    WHERE hash = ?1
                    ",
                    [&chunk_hash],
                )?;
            }
        }

        transaction.execute("DELETE FROM file_chunks WHERE file_id = ?1", [file_id])?;

        for (chunk_index, chunk_hash) in chunk_hashes.iter().enumerate() {
            transaction.execute(
                "
                INSERT INTO chunks (hash, reference_count)
                VALUES (?1, 1)
                ON CONFLICT(hash)
                DO UPDATE SET reference_count = reference_count + 1
                ",
                rusqlite::params![chunk_hash],
            )?;

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
    pub fn unreferenced_chunks(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "
            SELECT hash
            FROM chunks
            WHERE reference_count = 0
            ",
        )?;

        let chunks = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(chunks)
    }
    pub fn remove_chunk_record(&self, hash: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM chunks WHERE hash = ?1 AND reference_count = 0",
            [hash],
        )?;

        Ok(())
    }

    pub fn remove_file_manifest(&self, path: &Path) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        let file_id: i64 = transaction.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        {
            let mut statement = transaction.prepare(
                "
                SELECT chunk_hash
                FROM file_chunks
                WHERE file_id = ?1
                ",
            )?;

            let chunk_hashes = statement
                .query_map([file_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<String>, _>>()?;

            for chunk_hash in chunk_hashes {
                transaction.execute(
                    "
                    UPDATE chunks
                    SET reference_count = reference_count - 1
                    WHERE hash = ?1
                    ",
                    [&chunk_hash],
                )?;
            }
        }

        transaction.execute("DELETE FROM file_chunks WHERE file_id = ?1", [file_id])?;

        transaction.execute("DELETE FROM files WHERE id = ?1", [file_id])?;

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
    #[test]
    fn removes_file_manifest_and_updates_references() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        let chunks = vec!["hash-one".to_string(), "hash-two".to_string()];

        store
            .store_file_manifest(Path::new("test.txt"), &chunks)
            .unwrap();

        store.remove_file_manifest(Path::new("test.txt")).unwrap();

        let file_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();

        let manifest_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM file_chunks", [], |row| row.get(0))
            .unwrap();

        let references: i64 = store
            .connection
            .query_row("SELECT SUM(reference_count) FROM chunks", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(file_count, 0);
        assert_eq!(manifest_count, 0);
        assert_eq!(references, 0);
    }
    #[test]
    fn tracks_chunk_reference_count() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        let chunks = vec!["shared-hash".to_string()];

        store
            .store_file_manifest(Path::new("file-a.txt"), &chunks)
            .unwrap();

        store
            .store_file_manifest(Path::new("file-b.txt"), &chunks)
            .unwrap();

        let reference_count: i64 = store
            .connection
            .query_row(
                "SELECT reference_count FROM chunks WHERE hash = 'shared-hash'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(reference_count, 2);
    }
    #[test]
    fn replacing_manifest_updates_reference_counts() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        store
            .store_file_manifest(Path::new("test.txt"), &["old-hash".to_string()])
            .unwrap();

        store
            .replace_file_manifest(Path::new("test.txt"), &["new-hash".to_string()])
            .unwrap();

        let old_count: i64 = store
            .connection
            .query_row(
                "SELECT reference_count FROM chunks WHERE hash = 'old-hash'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let new_count: i64 = store
            .connection
            .query_row(
                "SELECT reference_count FROM chunks WHERE hash = 'new-hash'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(old_count, 0);
        assert_eq!(new_count, 1);
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
