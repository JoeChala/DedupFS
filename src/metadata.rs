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
}
