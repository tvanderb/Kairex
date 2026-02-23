use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use super::error::Result;
use super::models::Candle;

type CandleRow = (i64, f64, f64, f64, f64, f64);

/// Retention periods in milliseconds.
pub struct RetentionConfig {
    /// How long to keep 5m candles (e.g., 30 days)
    pub retain_5m_ms: i64,
    /// How long to keep 1h candles (e.g., 1 year)
    pub retain_1h_ms: i64,
    /// How long to keep 1d candles (e.g., 7 years)
    pub retain_1d_ms: i64,
}

impl RetentionConfig {
    pub fn default_config() -> Self {
        Self {
            retain_5m_ms: 30 * 24 * 3600 * 1000,      // 30 days
            retain_1h_ms: 365 * 24 * 3600 * 1000,     // 1 year
            retain_1d_ms: 7 * 365 * 24 * 3600 * 1000, // 7 years
        }
    }
}

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

/// Run the full retention cycle: aggregate then prune.
///
/// Ordering invariant: always aggregate before prune to avoid data loss.
pub fn run_retention(conn: &Connection, config: &RetentionConfig, now: i64) -> Result<()> {
    aggregate_candles(conn, "5m", "1h", HOUR_MS)?;
    aggregate_candles(conn, "1h", "1d", DAY_MS)?;
    prune_candles(conn, "5m", now - config.retain_5m_ms)?;
    prune_candles(conn, "1h", now - config.retain_1h_ms)?;
    prune_candles(conn, "1d", now - config.retain_1d_ms)?;
    Ok(())
}

/// Aggregate candles from a source timeframe into a target timeframe.
///
/// Groups source candles by symbol and target period, computes OHLCV:
/// - open = first candle's open (by open_time)
/// - close = last candle's close (by open_time)
/// - high = max of all highs
/// - low = min of all lows
/// - volume = sum of all volumes
///
/// Idempotent: skips if target candle already exists.
/// Partial periods are aggregated (gaps don't block aggregation).
pub fn aggregate_candles(
    conn: &Connection,
    source_tf: &str,
    target_tf: &str,
    period_ms: i64,
) -> Result<u64> {
    // Find all distinct symbols in source timeframe
    let mut symbols_stmt =
        conn.prepare("SELECT DISTINCT symbol FROM candles WHERE timeframe = ?1")?;
    let symbols: Vec<String> = symbols_stmt
        .query_map(params![source_tf], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;

    let mut aggregated = 0u64;

    for symbol in &symbols {
        aggregated += aggregate_symbol(conn, symbol, source_tf, target_tf, period_ms)?;
    }

    Ok(aggregated)
}

fn aggregate_symbol(
    conn: &Connection,
    symbol: &str,
    source_tf: &str,
    target_tf: &str,
    period_ms: i64,
) -> Result<u64> {
    // Get all source candles for this symbol, ordered by time
    let mut stmt = conn.prepare(
        "SELECT open_time, open, high, low, close, volume
         FROM candles
         WHERE symbol = ?1 AND timeframe = ?2
         ORDER BY open_time",
    )?;

    let rows: Vec<CandleRow> = stmt
        .query_map(params![symbol, source_tf], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;

    if rows.is_empty() {
        return Ok(0);
    }

    // Group by target period
    let mut groups: BTreeMap<i64, Vec<CandleRow>> = BTreeMap::new();

    for row in &rows {
        let period_start = (row.0 / period_ms) * period_ms;
        groups.entry(period_start).or_default().push(*row);
    }

    let mut count = 0u64;

    for (period_start, candles) in &groups {
        // Check if target already exists (idempotent)
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM candles WHERE symbol = ?1 AND timeframe = ?2 AND open_time = ?3)",
            params![symbol, target_tf, period_start],
            |row| row.get(0),
        )?;

        if exists {
            continue;
        }

        // Compute OHLCV
        let open = candles.first().unwrap().1;
        let close = candles.last().unwrap().4;
        let high = candles
            .iter()
            .map(|c| c.2)
            .fold(f64::NEG_INFINITY, f64::max);
        let low = candles.iter().map(|c| c.3).fold(f64::INFINITY, f64::min);
        let volume: f64 = candles.iter().map(|c| c.5).sum();

        let agg = Candle {
            symbol: symbol.to_string(),
            timeframe: target_tf.to_string(),
            open_time: *period_start,
            open,
            high,
            low,
            close,
            volume,
            source: "aggregated".to_string(),
        };

        conn.execute(
            "INSERT INTO candles (symbol, timeframe, open_time, open, high, low, close, volume, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                agg.symbol, agg.timeframe, agg.open_time,
                agg.open, agg.high, agg.low, agg.close, agg.volume, agg.source,
            ],
        )?;

        count += 1;
    }

    Ok(count)
}

fn prune_candles(conn: &Connection, timeframe: &str, cutoff: i64) -> Result<u64> {
    let deleted = conn.execute(
        "DELETE FROM candles WHERE timeframe = ?1 AND open_time < ?2",
        params![timeframe, cutoff],
    )?;
    Ok(deleted as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::market_data::insert_candles;
    use crate::storage::Database;

    fn test_db() -> (tempfile::TempDir, Database) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory(tmp.path()).unwrap();
        (tmp, db)
    }

    fn make_5m_candles(symbol: &str, count: usize, start: i64) -> Vec<Candle> {
        (0..count)
            .map(|i| Candle {
                symbol: symbol.into(),
                timeframe: "5m".into(),
                open_time: start + (i as i64) * 300_000,
                open: 100.0 + i as f64,
                high: 110.0 + i as f64,
                low: 90.0 + i as f64,
                close: 105.0 + i as f64,
                volume: 10.0,
                source: "ws".into(),
            })
            .collect()
    }

    fn make_1h_candles(symbol: &str, count: usize, start: i64) -> Vec<Candle> {
        (0..count)
            .map(|i| Candle {
                symbol: symbol.into(),
                timeframe: "1h".into(),
                open_time: start + (i as i64) * HOUR_MS,
                open: 100.0 + i as f64,
                high: 110.0 + i as f64,
                low: 90.0 + i as f64,
                close: 105.0 + i as f64,
                volume: 120.0,
                source: "aggregated".into(),
            })
            .collect()
    }

    #[test]
    fn aggregate_5m_to_1h() {
        let (_tmp, db) = test_db();
        // 12 five-minute candles = 1 full hour
        let candles = make_5m_candles("BTCUSDT", 12, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let count = db
            .with_writer(|conn| aggregate_candles(conn, "5m", "1h", HOUR_MS))
            .unwrap();
        assert_eq!(count, 1);

        // Verify aggregated candle
        db.with_reader(|conn| {
            let mut stmt = conn.prepare(
                "SELECT open, high, low, close, volume, source
                 FROM candles WHERE symbol = 'BTCUSDT' AND timeframe = '1h'",
            )?;
            let candle: (f64, f64, f64, f64, f64, String) = stmt.query_row([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?;
            // open = first candle's open
            assert_eq!(candle.0, 100.0);
            // high = max of all highs (110 + 11 = 121)
            assert_eq!(candle.1, 121.0);
            // low = min of all lows (90 + 0 = 90)
            assert_eq!(candle.2, 90.0);
            // close = last candle's close (105 + 11 = 116)
            assert_eq!(candle.3, 116.0);
            // volume = sum (10 * 12 = 120)
            assert_eq!(candle.4, 120.0);
            assert_eq!(candle.5, "aggregated");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn aggregate_partial_hour() {
        let (_tmp, db) = test_db();
        // Only 6 five-minute candles (half hour) — should still aggregate
        let candles = make_5m_candles("BTCUSDT", 6, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let count = db
            .with_writer(|conn| aggregate_candles(conn, "5m", "1h", HOUR_MS))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn aggregate_multiple_hours() {
        let (_tmp, db) = test_db();
        // 24 five-minute candles = 2 hours
        let candles = make_5m_candles("BTCUSDT", 24, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let count = db
            .with_writer(|conn| aggregate_candles(conn, "5m", "1h", HOUR_MS))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn aggregate_idempotent() {
        let (_tmp, db) = test_db();
        let candles = make_5m_candles("BTCUSDT", 12, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let count1 = db
            .with_writer(|conn| aggregate_candles(conn, "5m", "1h", HOUR_MS))
            .unwrap();
        assert_eq!(count1, 1);

        let count2 = db
            .with_writer(|conn| aggregate_candles(conn, "5m", "1h", HOUR_MS))
            .unwrap();
        assert_eq!(count2, 0); // Already exists, skip
    }

    #[test]
    fn aggregate_per_symbol_isolation() {
        let (_tmp, db) = test_db();
        let btc = make_5m_candles("BTCUSDT", 12, 0);
        let eth = make_5m_candles("ETHUSDT", 12, 0);
        db.with_writer(|conn| {
            insert_candles(conn, &btc)?;
            insert_candles(conn, &eth)
        })
        .unwrap();

        let count = db
            .with_writer(|conn| aggregate_candles(conn, "5m", "1h", HOUR_MS))
            .unwrap();
        assert_eq!(count, 2); // One per symbol

        // Each symbol has its own aggregated candle
        db.with_reader(|conn| {
            let agg_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '1h'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(agg_count, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn aggregate_1h_to_1d() {
        let (_tmp, db) = test_db();
        // 24 hourly candles = 1 day
        let candles = make_1h_candles("BTCUSDT", 24, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let count = db
            .with_writer(|conn| aggregate_candles(conn, "1h", "1d", DAY_MS))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn prune_old_candles() {
        let (_tmp, db) = test_db();
        let candles = make_5m_candles("BTCUSDT", 12, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        // Prune with cutoff at 1_800_000 (6 candles worth)
        let pruned = db
            .with_writer(|conn| prune_candles(conn, "5m", 1_800_000))
            .unwrap();
        assert_eq!(pruned, 6);

        db.with_reader(|conn| {
            let remaining: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '5m'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(remaining, 6);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn prune_respects_timeframe() {
        let (_tmp, db) = test_db();
        let fives = make_5m_candles("BTCUSDT", 12, 0);
        let hours = make_1h_candles("BTCUSDT", 2, 0);
        db.with_writer(|conn| {
            insert_candles(conn, &fives)?;
            insert_candles(conn, &hours)
        })
        .unwrap();

        // Prune 5m but not 1h
        db.with_writer(|conn| prune_candles(conn, "5m", i64::MAX))
            .unwrap();

        db.with_reader(|conn| {
            let fives_left: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '5m'",
                [],
                |row| row.get(0),
            )?;
            let hours_left: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '1h'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(fives_left, 0);
            assert_eq!(hours_left, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn run_retention_aggregate_before_prune() {
        let (_tmp, db) = test_db();

        // Insert 5m candles that would be pruned
        let candles = make_5m_candles("BTCUSDT", 12, 0);
        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        // run_retention with a config that prunes everything
        let config = RetentionConfig {
            retain_5m_ms: 0,
            retain_1h_ms: i64::MAX,
            retain_1d_ms: i64::MAX,
        };
        let now = HOUR_MS * 2; // well past the data

        db.with_writer(|conn| run_retention(conn, &config, now))
            .unwrap();

        // 5m candles should be pruned
        db.with_reader(|conn| {
            let fives: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '5m'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(fives, 0);
            Ok(())
        })
        .unwrap();

        // But 1h aggregation should have happened first
        db.with_reader(|conn| {
            let hours: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '1h'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(hours, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn full_retention_pipeline() {
        let (_tmp, db) = test_db();

        // 48 five-minute candles (4 hours) + some 1h candles for a full day
        let fives = make_5m_candles("BTCUSDT", 48, 0);
        let hours = make_1h_candles("BTCUSDT", 24, 0);
        db.with_writer(|conn| {
            insert_candles(conn, &fives)?;
            insert_candles(conn, &hours)
        })
        .unwrap();

        let config = RetentionConfig::default_config();
        let now = 2 * DAY_MS;

        db.with_writer(|conn| run_retention(conn, &config, now))
            .unwrap();

        // 5m→1h aggregated (4 hours), 1h→1d aggregated (1 day)
        // Nothing pruned with default retention (data is too recent)
        db.with_reader(|conn| {
            let daily: i64 = conn.query_row(
                "SELECT COUNT(*) FROM candles WHERE timeframe = '1d'",
                [],
                |row| row.get(0),
            )?;
            assert!(daily >= 1);
            Ok(())
        })
        .unwrap();
    }
}
