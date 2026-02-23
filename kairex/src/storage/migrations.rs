use rusqlite::Connection;

use super::error::{Result, StorageError};

struct Migration {
    version: i64,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("migrations/001_initial_schema.sql"),
}];

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for migration in MIGRATIONS {
        if migration.version > current_version {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(migration.sql).map_err(|e| {
                StorageError::Migration(format!("migration {} failed: {}", migration.version, e))
            })?;
            tx.execute(
                "INSERT INTO _migrations (version) VALUES (?1)",
                [migration.version],
            )?;
            tx.commit()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn tables_created() {
        let conn = memory_conn();
        run_migrations(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        assert!(tables.contains(&"candles".to_string()));
        assert!(tables.contains(&"funding_rates".to_string()));
        assert!(tables.contains(&"open_interest".to_string()));
        assert!(tables.contains(&"indices".to_string()));
        assert!(tables.contains(&"system_outputs".to_string()));
        assert!(tables.contains(&"active_setups".to_string()));
        assert!(tables.contains(&"fired_alerts".to_string()));
        assert!(tables.contains(&"_migrations".to_string()));
    }

    #[test]
    fn idempotent_rerun() {
        let conn = memory_conn();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn version_tracking() {
        let conn = memory_conn();
        run_migrations(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let version: i64 = conn
            .query_row("SELECT version FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }
}
