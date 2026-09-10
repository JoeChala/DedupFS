use std::path::Path;

use rusqlite::{Connection, Result};

pub struct MetadataStore {
    connection: Connection,
}

pub struct Snapshot {
    pub id: i64,
    pub name: String,
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
                path TEXT NOT NULL UNIQUE,
                current_version_id INTEGER
            );

            CREATE TABLE IF NOT EXISTS file_versions (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                FOREIGN KEY (file_id) REFERENCES files(id)
            );

            CREATE TABLE IF NOT EXISTS file_version_chunks (
                version_id INTEGER NOT NULL,
                chunk_hash TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                PRIMARY KEY (version_id, chunk_index),
                FOREIGN KEY (version_id) REFERENCES file_versions(id)
            );

            CREATE TABLE IF NOT EXISTS chunks (
                hash TEXT PRIMARY KEY,
                reference_count INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS snapshot_files (
                snapshot_id INTEGER NOT NULL,
                path TEXT NOT NULL,
                version_id INTEGER NOT NULL,
                PRIMARY KEY (snapshot_id, path),
                FOREIGN KEY (snapshot_id) REFERENCES snapshots(id),
                FOREIGN KEY (version_id) REFERENCES file_versions(id)
            );
            ",
        )?;

        Ok(())
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

    fn file_exists(&self, path: &Path) -> Result<bool> {
        let mut statement = self
            .connection
            .prepare("SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1)")?;

        statement.query_row([path.to_string_lossy().as_ref()], |row| row.get(0))
    }

    pub fn store_file_manifest(&self, path: &Path, chunk_hashes: &[String]) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        transaction.execute(
            "INSERT INTO files (path) VALUES (?1)",
            [path.to_string_lossy().as_ref()],
        )?;

        let file_id = transaction.last_insert_rowid();

        transaction.execute("INSERT INTO file_versions (file_id) VALUES (?1)", [file_id])?;

        let version_id = transaction.last_insert_rowid();

        for (chunk_index, chunk_hash) in chunk_hashes.iter().enumerate() {
            transaction.execute(
                "
                INSERT INTO file_version_chunks
                    (version_id, chunk_hash, chunk_index)
                VALUES (?1, ?2, ?3)
                ",
                rusqlite::params![version_id, chunk_hash, chunk_index as i64],
            )?;

            transaction.execute(
                "
                INSERT INTO chunks (hash, reference_count)
                VALUES (?1, 1)
                ON CONFLICT(hash)
                DO UPDATE SET reference_count = reference_count + 1
                ",
                [chunk_hash],
            )?;
        }

        transaction.execute(
            "UPDATE files SET current_version_id = ?1 WHERE id = ?2",
            rusqlite::params![version_id, file_id],
        )?;

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

        let old_version_id: i64 = transaction.query_row(
            "SELECT current_version_id FROM files WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )?;

        // The current file no longer references the old version.
        let old_hashes: Vec<String> = {
            let mut statement = transaction.prepare(
                "
                SELECT chunk_hash
                FROM file_version_chunks
                WHERE version_id = ?1
                ",
            )?;

            let rows = statement.query_map([old_version_id], |row| row.get(0))?;

            rows.collect::<Result<Vec<String>>>()?
        };

        for hash in old_hashes {
            transaction.execute(
                "
                UPDATE chunks
                SET reference_count = reference_count - 1
                WHERE hash = ?1
                ",
                [&hash],
            )?;
        }

        // Create the new immutable version.
        transaction.execute("INSERT INTO file_versions (file_id) VALUES (?1)", [file_id])?;

        let new_version_id = transaction.last_insert_rowid();

        for (chunk_index, chunk_hash) in chunk_hashes.iter().enumerate() {
            transaction.execute(
                "
                INSERT INTO file_version_chunks
                    (version_id, chunk_hash, chunk_index)
                VALUES (?1, ?2, ?3)
                ",
                rusqlite::params![new_version_id, chunk_hash, chunk_index as i64],
            )?;

            transaction.execute(
                "
                INSERT INTO chunks (hash, reference_count)
                VALUES (?1, 1)
                ON CONFLICT(hash)
                DO UPDATE SET reference_count = reference_count + 1
                ",
                [chunk_hash],
            )?;
        }

        transaction.execute(
            "UPDATE files SET current_version_id = ?1 WHERE id = ?2",
            rusqlite::params![new_version_id, file_id],
        )?;

        // If no snapshot refers to the old version anymore,
        // it is now dead and can be removed.
        let snapshot_references: i64 = transaction.query_row(
            "
            SELECT COUNT(*)
            FROM snapshot_files
            WHERE version_id = ?1
            ",
            [old_version_id],
            |row| row.get(0),
        )?;

        if snapshot_references == 0 {
            transaction.execute(
                "DELETE FROM file_version_chunks WHERE version_id = ?1",
                [old_version_id],
            )?;

            transaction.execute("DELETE FROM file_versions WHERE id = ?1", [old_version_id])?;
        }

        transaction.commit()?;

        Ok(())
    }

    pub fn get_file_manifest(&self, path: &Path) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "
            SELECT fvc.chunk_hash
            FROM files f
            JOIN file_version_chunks fvc
                ON f.current_version_id = fvc.version_id
            WHERE f.path = ?1
            ORDER BY fvc.chunk_index
            ",
        )?;

        let rows = statement.query_map([path.to_string_lossy().as_ref()], |row| row.get(0))?;

        rows.collect()
    }
    pub fn remove_file_manifest(&self, path: &Path) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        let file_id: i64 = transaction.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        let current_version_id: i64 = transaction.query_row(
            "SELECT current_version_id FROM files WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )?;

        let hashes: Vec<String> = {
            let mut statement = transaction.prepare(
                "
                SELECT chunk_hash
                FROM file_version_chunks
                WHERE version_id = ?1
                ",
            )?;

            let rows = statement.query_map([current_version_id], |row| row.get(0))?;

            rows.collect::<Result<Vec<String>>>()?
        };

        // The current file is no longer referencing this version.
        for hash in hashes {
            transaction.execute(
                "
                UPDATE chunks
                SET reference_count = reference_count - 1
                WHERE hash = ?1
                ",
                [&hash],
            )?;
        }

        let snapshot_references: i64 = transaction.query_row(
            "
            SELECT COUNT(*)
            FROM snapshot_files
            WHERE version_id = ?1
            ",
            [current_version_id],
            |row| row.get(0),
        )?;

        if snapshot_references == 0 {
            transaction.execute(
                "DELETE FROM file_version_chunks WHERE version_id = ?1",
                [current_version_id],
            )?;

            transaction.execute(
                "DELETE FROM file_versions WHERE id = ?1",
                [current_version_id],
            )?;
        }

        transaction.execute("DELETE FROM files WHERE id = ?1", [file_id])?;

        transaction.commit()?;

        Ok(())
    }

    pub fn unreferenced_chunks(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "
            SELECT hash
            FROM chunks
            WHERE reference_count = 0
            ",
        )?;

        let rows = statement.query_map([], |row| row.get(0))?;

        rows.collect()
    }

    pub fn remove_chunk_record(&self, hash: &str) -> Result<()> {
        self.connection.execute(
            "
            DELETE FROM chunks
            WHERE hash = ?1
              AND reference_count = 0
            ",
            [hash],
        )?;

        Ok(())
    }
    pub fn create_snapshot(&self, name: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        transaction.execute("INSERT INTO snapshots (name) VALUES (?1)", [name])?;

        let snapshot_id = transaction.last_insert_rowid();

        let files: Vec<(String, i64)> = {
            let mut statement = transaction.prepare(
                "
                SELECT path, current_version_id
                FROM files
                WHERE current_version_id IS NOT NULL
                ",
            )?;

            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

            rows.collect::<Result<Vec<(String, i64)>>>()?
        };

        for (path, version_id) in files {
            transaction.execute(
                "
                INSERT INTO snapshot_files
                    (snapshot_id, path, version_id)
                VALUES (?1, ?2, ?3)
                ",
                rusqlite::params![snapshot_id, path, version_id],
            )?;

            let hashes: Vec<String> = {
                let mut statement = transaction.prepare(
                    "
                    SELECT chunk_hash
                    FROM file_version_chunks
                    WHERE version_id = ?1
                    ",
                )?;

                let rows = statement.query_map([version_id], |row| row.get(0))?;

                rows.collect::<Result<Vec<String>>>()?
            };

            // Snapshot now holds a reference to this version's chunks.
            for hash in hashes {
                transaction.execute(
                    "
                    UPDATE chunks
                    SET reference_count = reference_count + 1
                    WHERE hash = ?1
                    ",
                    [&hash],
                )?;
            }
        }

        transaction.commit()?;

        Ok(())
    }

    pub fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        let mut statement = self.connection.prepare(
            "
            SELECT id, name
            FROM snapshots
            ORDER BY id
            ",
        )?;

        let rows = statement.query_map([], |row| {
            Ok(Snapshot {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;

        rows.collect()
    }
    pub fn delete_snapshot(&self, name: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;

        let snapshot_id: i64 =
            transaction.query_row("SELECT id FROM snapshots WHERE name = ?1", [name], |row| {
                row.get(0)
            })?;

        // Remember which versions this snapshot references.
        let version_ids: Vec<i64> = {
            let mut statement = transaction.prepare(
                "
                SELECT version_id
                FROM snapshot_files
                WHERE snapshot_id = ?1
                ",
            )?;

            let rows = statement.query_map([snapshot_id], |row| row.get(0))?;

            rows.collect::<Result<Vec<i64>>>()?
        };

        // Remove this snapshot's references first.
        //
        // This must happen before deleting file_versions because
        // snapshot_files.version_id is a foreign key to file_versions.id.
        transaction.execute(
            "DELETE FROM snapshot_files WHERE snapshot_id = ?1",
            [snapshot_id],
        )?;

        for version_id in version_ids {
            let hashes: Vec<String> = {
                let mut statement = transaction.prepare(
                    "
                    SELECT chunk_hash
                    FROM file_version_chunks
                    WHERE version_id = ?1
                    ",
                )?;

                let rows = statement.query_map([version_id], |row| row.get(0))?;

                rows.collect::<Result<Vec<String>>>()?
            };

            // The deleted snapshot no longer references these chunks.
            for hash in hashes {
                transaction.execute(
                    "
                    UPDATE chunks
                    SET reference_count = reference_count - 1
                    WHERE hash = ?1
                    ",
                    [&hash],
                )?;
            }

            // Check whether the version is still the current version
            // of any file.
            let current_reference_count: i64 = transaction.query_row(
                "
                SELECT COUNT(*)
                FROM files
                WHERE current_version_id = ?1
                ",
                [version_id],
                |row| row.get(0),
            )?;

            // Check whether another snapshot still references it.
            let remaining_snapshot_references: i64 = transaction.query_row(
                "
                SELECT COUNT(*)
                FROM snapshot_files
                WHERE version_id = ?1
                ",
                [version_id],
                |row| row.get(0),
            )?;

            // If nothing references this version anymore, delete the
            // version metadata.
            if current_reference_count == 0 && remaining_snapshot_references == 0 {
                transaction.execute(
                    "DELETE FROM file_version_chunks WHERE version_id = ?1",
                    [version_id],
                )?;

                transaction.execute("DELETE FROM file_versions WHERE id = ?1", [version_id])?;
            }
        }

        transaction.execute("DELETE FROM snapshots WHERE id = ?1", [snapshot_id])?;

        transaction.commit()?;

        Ok(())
    }

    pub fn get_snapshot_manifest(&self, name: &str, path: &Path) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "
            SELECT fvc.chunk_hash
            FROM snapshots s
            JOIN snapshot_files sf
                ON s.id = sf.snapshot_id
            JOIN file_version_chunks fvc
                ON sf.version_id = fvc.version_id
            WHERE s.name = ?1
            AND sf.path = ?2
            ORDER BY fvc.chunk_index
            ",
        )?;

        let rows = statement.query_map(
            rusqlite::params![name, path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;

        rows.collect()
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
            .query_row("SELECT COUNT(*) FROM file_version_chunks", [], |row| {
                row.get(0)
            })
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
            .query_row("SELECT COUNT(*) FROM file_version_chunks", [], |row| {
                row.get(0)
            })
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
    fn snapshot_keeps_old_version_alive() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        store
            .store_file_manifest(Path::new("test.txt"), &["old-hash".to_string()])
            .unwrap();

        store.create_snapshot("first").unwrap();

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

        assert_eq!(old_count, 1);
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
            .query_row("SELECT COUNT(*) FROM file_version_chunks", [], |row| {
                row.get(0)
            })
            .unwrap();

        let stored_hash: String = store
            .connection
            .query_row(
                "
                    SELECT chunk_hash
                    FROM file_version_chunks
                    WHERE version_id = (
                        SELECT current_version_id
                        FROM files
                        WHERE id = 1
                    )
                    ",
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
    #[test]
    fn deleting_snapshot_releases_chunk_references() {
        let database_path = PathBuf::from(":memory:");

        let store = MetadataStore::open(&database_path).unwrap();
        store.initialize().unwrap();

        store
            .store_file_manifest(Path::new("test.txt"), &["hash-one".to_string()])
            .unwrap();

        store.create_snapshot("first").unwrap();

        let before_delete: i64 = store
            .connection
            .query_row(
                "SELECT reference_count FROM chunks WHERE hash = 'hash-one'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(before_delete, 2);

        store.delete_snapshot("first").unwrap();

        let after_delete: i64 = store
            .connection
            .query_row(
                "SELECT reference_count FROM chunks WHERE hash = 'hash-one'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(after_delete, 1);
    }
}
