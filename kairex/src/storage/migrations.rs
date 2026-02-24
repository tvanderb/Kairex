use rusqlite::Connection;

use super::error::{Result, StorageError};

struct Migration {
    version: i64,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("migrations/001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/002_add_setup_fields.sql"),
    },
];

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
        assert_eq!(version, 2);
    }

    #[test]
    fn version_tracking() {
        let conn = memory_conn();
        run_migrations(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    /// Insert a dummy system_output row so FK constraints are satisfied.
    fn insert_dummy_output(conn: &Connection) {
        conn.execute(
            "INSERT INTO system_outputs (report_type, generated_at, schema_version, output)
             VALUES ('test', 1000, 'v1', '{}')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn migration_002_adds_setup_columns() {
        let conn = memory_conn();
        run_migrations(&conn).unwrap();
        insert_dummy_output(&conn);

        // Verify trigger_field and confidence columns exist on active_setups
        conn.execute(
            "INSERT INTO active_setups
                (source_output_id, asset, direction, trigger_condition, trigger_level,
                 trigger_field, confidence, status, created_at)
             VALUES (1, 'BTCUSDT', 'long', 'indicator_below', 30.0,
                     'rsi_14_1h', 0.85, 'active', 1000)",
            [],
        )
        .unwrap();

        let (trigger_field, confidence): (Option<String>, Option<f64>) = conn
            .query_row(
                "SELECT trigger_field, confidence FROM active_setups WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(trigger_field.as_deref(), Some("rsi_14_1h"));
        assert_eq!(confidence, Some(0.85));
    }

    #[test]
    fn migration_002_columns_nullable() {
        let conn = memory_conn();
        run_migrations(&conn).unwrap();
        insert_dummy_output(&conn);

        // Verify the new columns are nullable (null for price triggers)
        conn.execute(
            "INSERT INTO active_setups
                (source_output_id, asset, direction, trigger_condition, trigger_level,
                 status, created_at)
             VALUES (1, 'BTCUSDT', 'long', 'price_above', 70000.0, 'active', 1000)",
            [],
        )
        .unwrap();

        let (trigger_field, confidence): (Option<String>, Option<f64>) = conn
            .query_row(
                "SELECT trigger_field, confidence FROM active_setups WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(trigger_field.is_none());
        assert!(confidence.is_none());
    }

    #[test]
    fn incremental_migration_from_v1() {
        let conn = memory_conn();

        // Run only migration 1
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .unwrap();
        conn.execute_batch(MIGRATIONS[0].sql).unwrap();
        conn.execute("INSERT INTO _migrations (version) VALUES (?1)", [1])
            .unwrap();

        // Now run all migrations — only v2 should apply
        run_migrations(&conn).unwrap();

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);

        // Verify v2 columns exist — insert dummy output first for FK
        insert_dummy_output(&conn);
        conn.execute(
            "INSERT INTO active_setups
                (source_output_id, asset, direction, trigger_condition, trigger_level,
                 trigger_field, confidence, status, created_at)
             VALUES (1, 'BTCUSDT', 'long', 'price_above', 70000.0,
                     'rsi_14_1h', 0.7, 'active', 1000)",
            [],
        )
        .unwrap();
    }
}
