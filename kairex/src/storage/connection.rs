use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::error::Result;
use super::migrations::run_migrations;

#[derive(Clone)]
pub struct Database {
    writer: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        run_migrations(&conn)?;

        Ok(Self {
            writer: Arc::new(Mutex::new(conn)),
            path: path.to_path_buf(),
        })
    }

    /// Open a temp-file-backed database for tests.
    ///
    /// Uses a real file so WAL mode and multiple connections work correctly.
    pub fn open_in_memory(temp_dir: &Path) -> Result<Self> {
        let path = temp_dir.join("kairex_test.db");
        Self::open(&path)
    }

    pub fn with_writer<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let conn = self.writer.lock().expect("writer lock poisoned");
        f(&conn)
    }

    pub fn with_reader<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let conn = Connection::open(&self.path)?;
        Self::configure(&conn)?;
        f(&conn)
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_mode_active() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory(tmp.path()).unwrap();

        db.with_reader(|conn| {
            let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
            assert_eq!(mode.to_lowercase(), "wal");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn concurrent_read_write() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory(tmp.path()).unwrap();

        // Write a candle
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO candles (symbol, timeframe, open_time, open, high, low, close, volume, source)
                 VALUES ('BTCUSDT', '5m', 1000, 100.0, 110.0, 90.0, 105.0, 1000.0, 'test')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        // Read concurrently via a separate connection
        db.with_reader(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM candles", [], |row| row.get(0))?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn migrations_run_on_open() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory(tmp.path()).unwrap();

        db.with_reader(|conn| {
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?
                .query_map([], |row| row.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            assert!(tables.contains(&"candles".to_string()));
            assert!(tables.contains(&"system_outputs".to_string()));
            Ok(())
        })
        .unwrap();
    }
}
