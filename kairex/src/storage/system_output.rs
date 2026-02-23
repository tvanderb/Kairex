use rusqlite::{params, Connection};

use super::error::Result;
use super::models::{ActiveSetup, FiredAlert, SystemOutput};

/// Store a report and its associated setups in a single transaction.
///
/// 1. Insert the system output row
/// 2. Supersede previous active setups for the same assets
/// 3. Expire active setups for assets not in the new report
/// 4. Insert new setups
pub fn store_report(
    conn: &Connection,
    output: &SystemOutput,
    setups: &[ActiveSetup],
) -> Result<i64> {
    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "INSERT INTO system_outputs (report_type, generated_at, schema_version, output, delivered_at, delivery_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            output.report_type,
            output.generated_at,
            output.schema_version,
            output.output.to_string(),
            output.delivered_at,
            output.delivery_status,
        ],
    )?;

    let output_id = tx.last_insert_rowid();

    // Collect new setup assets for supersede/expire logic
    let new_assets: Vec<&str> = setups.iter().map(|s| s.asset.as_str()).collect();

    // Supersede previous active setups for assets that appear in new setups
    for asset in &new_assets {
        tx.execute(
            "UPDATE active_setups SET status = 'superseded', resolved_at = ?1
             WHERE asset = ?2 AND status = 'active'",
            params![output.generated_at, asset],
        )?;
    }

    // Expire active setups for assets NOT in the new report
    // (only if this is a scheduled report, not an alert)
    if output.report_type != "alert" && !new_assets.is_empty() {
        let placeholders: Vec<String> = (0..new_assets.len())
            .map(|i| format!("?{}", i + 2))
            .collect();
        let sql = format!(
            "UPDATE active_setups SET status = 'expired', resolved_at = ?1
             WHERE status = 'active' AND asset NOT IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = tx.prepare(&sql)?;
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(output.generated_at)];
        for asset in &new_assets {
            param_values.push(Box::new(asset.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        stmt.execute(param_refs.as_slice())?;
    }

    // Insert new setups
    for setup in setups {
        tx.execute(
            "INSERT INTO active_setups
                (source_output_id, asset, direction, trigger_condition, trigger_level,
                 target_level, invalidation_level, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', ?8)",
            params![
                output_id,
                setup.asset,
                setup.direction,
                setup.trigger_condition,
                setup.trigger_level,
                setup.target_level,
                setup.invalidation_level,
                setup.created_at,
            ],
        )?;
    }

    tx.commit()?;
    Ok(output_id)
}

pub fn query_outputs_by_type(
    conn: &Connection,
    report_type: &str,
    limit: i64,
) -> Result<Vec<SystemOutput>> {
    let mut stmt = conn.prepare(
        "SELECT id, report_type, generated_at, schema_version, output, delivered_at, delivery_status
         FROM system_outputs
         WHERE report_type = ?1
         ORDER BY generated_at DESC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![report_type, limit], row_to_output)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn query_outputs_by_date_range(
    conn: &Connection,
    start: i64,
    end: i64,
) -> Result<Vec<SystemOutput>> {
    let mut stmt = conn.prepare(
        "SELECT id, report_type, generated_at, schema_version, output, delivered_at, delivery_status
         FROM system_outputs
         WHERE generated_at >= ?1 AND generated_at < ?2
         ORDER BY generated_at",
    )?;

    let rows = stmt.query_map(params![start, end], row_to_output)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn query_latest_output_by_type(
    conn: &Connection,
    report_type: &str,
) -> Result<Option<SystemOutput>> {
    let results = query_outputs_by_type(conn, report_type, 1)?;
    Ok(results.into_iter().next())
}

pub fn resolve_setup(
    conn: &Connection,
    setup_id: i64,
    status: &str,
    resolved_at: i64,
    resolved_price: f64,
) -> Result<()> {
    conn.execute(
        "UPDATE active_setups SET status = ?1, resolved_at = ?2, resolved_price = ?3
         WHERE id = ?4 AND status = 'active'",
        params![status, resolved_at, resolved_price, setup_id],
    )?;
    Ok(())
}

pub fn update_delivery_status(
    conn: &Connection,
    output_id: i64,
    status: &str,
    delivered_at: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE system_outputs SET delivery_status = ?1, delivered_at = ?2
         WHERE id = ?3",
        params![status, delivered_at, output_id],
    )?;
    Ok(())
}

pub fn query_active_setups(conn: &Connection) -> Result<Vec<ActiveSetup>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_output_id, asset, direction, trigger_condition, trigger_level,
                target_level, invalidation_level, status, created_at, resolved_at, resolved_price
         FROM active_setups
         WHERE status = 'active'
         ORDER BY created_at",
    )?;

    let rows = stmt.query_map([], row_to_setup)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn query_active_setups_by_asset(conn: &Connection, asset: &str) -> Result<Vec<ActiveSetup>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_output_id, asset, direction, trigger_condition, trigger_level,
                target_level, invalidation_level, status, created_at, resolved_at, resolved_price
         FROM active_setups
         WHERE asset = ?1 AND status = 'active'
         ORDER BY created_at",
    )?;

    let rows = stmt.query_map(params![asset], row_to_setup)?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn insert_fired_alert(conn: &Connection, alert: &FiredAlert) -> Result<i64> {
    conn.execute(
        "INSERT INTO fired_alerts (setup_id, alert_type, fired_at, cooldown_until, output_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            alert.setup_id,
            alert.alert_type,
            alert.fired_at,
            alert.cooldown_until,
            alert.output_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn is_alert_on_cooldown(conn: &Connection, alert_type: &str, now: i64) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM fired_alerts
         WHERE alert_type = ?1 AND cooldown_until > ?2",
        params![alert_type, now],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn row_to_output(row: &rusqlite::Row) -> rusqlite::Result<SystemOutput> {
    let output_str: String = row.get(4)?;
    let output: serde_json::Value =
        serde_json::from_str(&output_str).unwrap_or(serde_json::Value::Null);
    Ok(SystemOutput {
        id: Some(row.get(0)?),
        report_type: row.get(1)?,
        generated_at: row.get(2)?,
        schema_version: row.get(3)?,
        output,
        delivered_at: row.get(5)?,
        delivery_status: row.get(6)?,
    })
}

fn row_to_setup(row: &rusqlite::Row) -> rusqlite::Result<ActiveSetup> {
    Ok(ActiveSetup {
        id: Some(row.get(0)?),
        source_output_id: row.get(1)?,
        asset: row.get(2)?,
        direction: row.get(3)?,
        trigger_condition: row.get(4)?,
        trigger_level: row.get(5)?,
        target_level: row.get(6)?,
        invalidation_level: row.get(7)?,
        status: row.get(8)?,
        created_at: row.get(9)?,
        resolved_at: row.get(10)?,
        resolved_price: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Database;

    fn test_db() -> (tempfile::TempDir, Database) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory(tmp.path()).unwrap();
        (tmp, db)
    }

    fn make_output(report_type: &str, generated_at: i64) -> SystemOutput {
        SystemOutput {
            id: None,
            report_type: report_type.into(),
            generated_at,
            schema_version: "v1".into(),
            output: serde_json::json!({"test": true}),
            delivered_at: None,
            delivery_status: "pending".into(),
        }
    }

    fn make_setup(asset: &str, created_at: i64) -> ActiveSetup {
        ActiveSetup {
            id: None,
            source_output_id: 0, // filled by store_report
            asset: asset.into(),
            direction: "long".into(),
            trigger_condition: "price_above".into(),
            trigger_level: 70000.0,
            target_level: Some(75000.0),
            invalidation_level: Some(65000.0),
            status: "active".into(),
            created_at,
            resolved_at: None,
            resolved_price: None,
        }
    }

    #[test]
    fn store_and_query_output() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);

        let id = db
            .with_writer(|conn| store_report(conn, &output, &[]))
            .unwrap();
        assert!(id > 0);

        let results = db
            .with_reader(|conn| query_outputs_by_type(conn, "morning", 10))
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, Some(id));
        assert_eq!(results[0].report_type, "morning");
    }

    #[test]
    fn store_report_with_setups() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);
        let setups = vec![make_setup("BTCUSDT", 1000), make_setup("ETHUSDT", 1000)];

        db.with_writer(|conn| store_report(conn, &output, &setups))
            .unwrap();

        let active = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn supersede_previous_setups() {
        let (_tmp, db) = test_db();

        // First report: BTC + ETH setups
        let output1 = make_output("morning", 1000);
        let setups1 = vec![make_setup("BTCUSDT", 1000), make_setup("ETHUSDT", 1000)];
        db.with_writer(|conn| store_report(conn, &output1, &setups1))
            .unwrap();

        // Second report: new BTC setup (supersedes old BTC, expires ETH)
        let output2 = make_output("midday", 2000);
        let setups2 = vec![make_setup("BTCUSDT", 2000)];
        db.with_writer(|conn| store_report(conn, &output2, &setups2))
            .unwrap();

        let active = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].asset, "BTCUSDT");
        assert_eq!(active[0].created_at, 2000);
    }

    #[test]
    fn expire_dropped_assets() {
        let (_tmp, db) = test_db();

        // Report with BTC + ETH + SOL
        let output1 = make_output("morning", 1000);
        let setups1 = vec![
            make_setup("BTCUSDT", 1000),
            make_setup("ETHUSDT", 1000),
            make_setup("SOLUSDT", 1000),
        ];
        db.with_writer(|conn| store_report(conn, &output1, &setups1))
            .unwrap();

        // Next report only has BTC — ETH and SOL should be expired
        let output2 = make_output("midday", 2000);
        let setups2 = vec![make_setup("BTCUSDT", 2000)];
        db.with_writer(|conn| store_report(conn, &output2, &setups2))
            .unwrap();

        let active = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].asset, "BTCUSDT");
    }

    #[test]
    fn resolve_setup_triggered() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);
        let setups = vec![make_setup("BTCUSDT", 1000)];
        db.with_writer(|conn| store_report(conn, &output, &setups))
            .unwrap();

        let active = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        let setup_id = active[0].id.unwrap();

        db.with_writer(|conn| resolve_setup(conn, setup_id, "triggered", 1500, 71000.0))
            .unwrap();

        let remaining = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn delivery_status_update() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);
        let id = db
            .with_writer(|conn| store_report(conn, &output, &[]))
            .unwrap();

        db.with_writer(|conn| update_delivery_status(conn, id, "delivered", 1100))
            .unwrap();

        let results = db
            .with_reader(|conn| query_outputs_by_type(conn, "morning", 10))
            .unwrap();
        assert_eq!(results[0].delivery_status, "delivered");
        assert_eq!(results[0].delivered_at, Some(1100));
    }

    #[test]
    fn query_by_date_range() {
        let (_tmp, db) = test_db();
        for i in 0..5 {
            let output = make_output("morning", i * 1000);
            db.with_writer(|conn| store_report(conn, &output, &[]))
                .unwrap();
        }

        let results = db
            .with_reader(|conn| query_outputs_by_date_range(conn, 1000, 3000))
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].generated_at, 1000);
        assert_eq!(results[1].generated_at, 2000);
    }

    #[test]
    fn latest_output_by_type() {
        let (_tmp, db) = test_db();
        for i in 0..3 {
            let output = make_output("evening", i * 1000);
            db.with_writer(|conn| store_report(conn, &output, &[]))
                .unwrap();
        }

        let latest = db
            .with_reader(|conn| query_latest_output_by_type(conn, "evening"))
            .unwrap()
            .unwrap();
        assert_eq!(latest.generated_at, 2000);
    }

    #[test]
    fn active_setups_by_asset() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);
        let setups = vec![make_setup("BTCUSDT", 1000), make_setup("ETHUSDT", 1000)];
        db.with_writer(|conn| store_report(conn, &output, &setups))
            .unwrap();

        let btc = db
            .with_reader(|conn| query_active_setups_by_asset(conn, "BTCUSDT"))
            .unwrap();
        assert_eq!(btc.len(), 1);
        assert_eq!(btc[0].asset, "BTCUSDT");
    }

    #[test]
    fn fired_alert_and_cooldown() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);
        let setups = vec![make_setup("BTCUSDT", 1000)];
        let output_id = db
            .with_writer(|conn| store_report(conn, &output, &setups))
            .unwrap();

        let active = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        let setup_id = active[0].id.unwrap();

        let alert = FiredAlert {
            id: None,
            setup_id: Some(setup_id),
            alert_type: "setup_trigger".into(),
            fired_at: 1500,
            cooldown_until: 2500,
            output_id: Some(output_id),
        };
        db.with_writer(|conn| insert_fired_alert(conn, &alert))
            .unwrap();

        // Should be on cooldown at t=2000
        let on_cooldown = db
            .with_reader(|conn| is_alert_on_cooldown(conn, "setup_trigger", 2000))
            .unwrap();
        assert!(on_cooldown);

        // Should not be on cooldown at t=3000
        let off_cooldown = db
            .with_reader(|conn| is_alert_on_cooldown(conn, "setup_trigger", 3000))
            .unwrap();
        assert!(!off_cooldown);
    }

    #[test]
    fn alerts_dont_expire_other_setups() {
        let (_tmp, db) = test_db();

        // Report with BTC + ETH setups
        let output1 = make_output("morning", 1000);
        let setups1 = vec![make_setup("BTCUSDT", 1000), make_setup("ETHUSDT", 1000)];
        db.with_writer(|conn| store_report(conn, &output1, &setups1))
            .unwrap();

        // Alert report for just BTC — should NOT expire ETH
        let alert = make_output("alert", 1500);
        let alert_setups = vec![make_setup("BTCUSDT", 1500)];
        db.with_writer(|conn| store_report(conn, &alert, &alert_setups))
            .unwrap();

        let active = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        // BTC superseded+new, ETH still active
        assert_eq!(active.len(), 2);
        let assets: Vec<&str> = active.iter().map(|s| s.asset.as_str()).collect();
        assert!(assets.contains(&"BTCUSDT"));
        assert!(assets.contains(&"ETHUSDT"));
    }

    #[test]
    fn transaction_atomicity() {
        let (_tmp, db) = test_db();
        let output = make_output("morning", 1000);

        // Store report should be atomic — setups are only visible after commit
        let id = db
            .with_writer(|conn| {
                let setups = vec![make_setup("BTCUSDT", 1000)];
                store_report(conn, &output, &setups)
            })
            .unwrap();
        assert!(id > 0);

        // Both output and setup should be present
        let outputs = db
            .with_reader(|conn| query_outputs_by_type(conn, "morning", 10))
            .unwrap();
        let setups = db.with_reader(|conn| query_active_setups(conn)).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(setups.len(), 1);
    }
}
