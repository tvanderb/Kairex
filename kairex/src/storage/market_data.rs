use rusqlite::{params, Connection};

use super::error::Result;
use super::models::{Candle, FundingRate, IndexValue, OpenInterest};

// -- Candles --

pub fn insert_candle(conn: &Connection, candle: &Candle) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO candles
            (symbol, timeframe, open_time, open, high, low, close, volume, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            candle.symbol,
            candle.timeframe,
            candle.open_time,
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume,
            candle.source,
        ],
    )?;
    Ok(())
}

pub fn insert_candles(conn: &Connection, candles: &[Candle]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for candle in candles {
        tx.execute(
            "INSERT OR REPLACE INTO candles
                (symbol, timeframe, open_time, open, high, low, close, volume, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                candle.symbol,
                candle.timeframe,
                candle.open_time,
                candle.open,
                candle.high,
                candle.low,
                candle.close,
                candle.volume,
                candle.source,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn query_candles(
    conn: &Connection,
    symbol: &str,
    timeframe: &str,
    start: i64,
    end: i64,
) -> Result<Vec<Candle>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, timeframe, open_time, open, high, low, close, volume, source
         FROM candles
         WHERE symbol = ?1 AND timeframe = ?2 AND open_time >= ?3 AND open_time < ?4
         ORDER BY open_time",
    )?;

    let rows = stmt.query_map(params![symbol, timeframe, start, end], |row| {
        Ok(Candle {
            symbol: row.get(0)?,
            timeframe: row.get(1)?,
            open_time: row.get(2)?,
            open: row.get(3)?,
            high: row.get(4)?,
            low: row.get(5)?,
            close: row.get(6)?,
            volume: row.get(7)?,
            source: row.get(8)?,
        })
    })?;

    let mut candles = Vec::new();
    for row in rows {
        candles.push(row?);
    }
    Ok(candles)
}

pub fn query_latest_candle(
    conn: &Connection,
    symbol: &str,
    timeframe: &str,
) -> Result<Option<Candle>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, timeframe, open_time, open, high, low, close, volume, source
         FROM candles
         WHERE symbol = ?1 AND timeframe = ?2
         ORDER BY open_time DESC LIMIT 1",
    )?;

    let mut rows = stmt.query_map(params![symbol, timeframe], |row| {
        Ok(Candle {
            symbol: row.get(0)?,
            timeframe: row.get(1)?,
            open_time: row.get(2)?,
            open: row.get(3)?,
            high: row.get(4)?,
            low: row.get(5)?,
            close: row.get(6)?,
            volume: row.get(7)?,
            source: row.get(8)?,
        })
    })?;

    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

// -- Funding Rates --

pub fn insert_funding_rate(conn: &Connection, rate: &FundingRate) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO funding_rates (symbol, timestamp, rate)
         VALUES (?1, ?2, ?3)",
        params![rate.symbol, rate.timestamp, rate.rate],
    )?;
    Ok(())
}

pub fn query_funding_rates(
    conn: &Connection,
    symbol: &str,
    start: i64,
    end: i64,
) -> Result<Vec<FundingRate>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, timestamp, rate
         FROM funding_rates
         WHERE symbol = ?1 AND timestamp >= ?2 AND timestamp < ?3
         ORDER BY timestamp",
    )?;

    let rows = stmt.query_map(params![symbol, start, end], |row| {
        Ok(FundingRate {
            symbol: row.get(0)?,
            timestamp: row.get(1)?,
            rate: row.get(2)?,
        })
    })?;

    let mut rates = Vec::new();
    for row in rows {
        rates.push(row?);
    }
    Ok(rates)
}

pub fn query_latest_funding_rate(conn: &Connection, symbol: &str) -> Result<Option<FundingRate>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, timestamp, rate
         FROM funding_rates
         WHERE symbol = ?1
         ORDER BY timestamp DESC LIMIT 1",
    )?;

    let mut rows = stmt.query_map(params![symbol], |row| {
        Ok(FundingRate {
            symbol: row.get(0)?,
            timestamp: row.get(1)?,
            rate: row.get(2)?,
        })
    })?;

    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

// -- Open Interest --

pub fn insert_open_interest(conn: &Connection, oi: &OpenInterest) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO open_interest (symbol, timestamp, value)
         VALUES (?1, ?2, ?3)",
        params![oi.symbol, oi.timestamp, oi.value],
    )?;
    Ok(())
}

pub fn query_open_interest(
    conn: &Connection,
    symbol: &str,
    start: i64,
    end: i64,
) -> Result<Vec<OpenInterest>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, timestamp, value
         FROM open_interest
         WHERE symbol = ?1 AND timestamp >= ?2 AND timestamp < ?3
         ORDER BY timestamp",
    )?;

    let rows = stmt.query_map(params![symbol, start, end], |row| {
        Ok(OpenInterest {
            symbol: row.get(0)?,
            timestamp: row.get(1)?,
            value: row.get(2)?,
        })
    })?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

pub fn query_latest_open_interest(conn: &Connection, symbol: &str) -> Result<Option<OpenInterest>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, timestamp, value
         FROM open_interest
         WHERE symbol = ?1
         ORDER BY timestamp DESC LIMIT 1",
    )?;

    let mut rows = stmt.query_map(params![symbol], |row| {
        Ok(OpenInterest {
            symbol: row.get(0)?,
            timestamp: row.get(1)?,
            value: row.get(2)?,
        })
    })?;

    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

// -- Indices --

pub fn insert_index_value(conn: &Connection, idx: &IndexValue) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO indices (index_type, timestamp, value)
         VALUES (?1, ?2, ?3)",
        params![idx.index_type, idx.timestamp, idx.value],
    )?;
    Ok(())
}

pub fn query_index_values(
    conn: &Connection,
    index_type: &str,
    start: i64,
    end: i64,
) -> Result<Vec<IndexValue>> {
    let mut stmt = conn.prepare(
        "SELECT index_type, timestamp, value
         FROM indices
         WHERE index_type = ?1 AND timestamp >= ?2 AND timestamp < ?3
         ORDER BY timestamp",
    )?;

    let rows = stmt.query_map(params![index_type, start, end], |row| {
        Ok(IndexValue {
            index_type: row.get(0)?,
            timestamp: row.get(1)?,
            value: row.get(2)?,
        })
    })?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

pub fn query_latest_index_value(conn: &Connection, index_type: &str) -> Result<Option<IndexValue>> {
    let mut stmt = conn.prepare(
        "SELECT index_type, timestamp, value
         FROM indices
         WHERE index_type = ?1
         ORDER BY timestamp DESC LIMIT 1",
    )?;

    let mut rows = stmt.query_map(params![index_type], |row| {
        Ok(IndexValue {
            index_type: row.get(0)?,
            timestamp: row.get(1)?,
            value: row.get(2)?,
        })
    })?;

    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
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

    #[test]
    fn candle_insert_query_roundtrip() {
        let (_tmp, db) = test_db();
        let candle = Candle {
            symbol: "BTCUSDT".into(),
            timeframe: "5m".into(),
            open_time: 1000,
            open: 100.0,
            high: 110.0,
            low: 90.0,
            close: 105.0,
            volume: 500.0,
            source: "ws".into(),
        };

        db.with_writer(|conn| insert_candle(conn, &candle)).unwrap();

        let result = db
            .with_reader(|conn| query_candles(conn, "BTCUSDT", "5m", 0, 2000))
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], candle);
    }

    #[test]
    fn candle_upsert_behavior() {
        let (_tmp, db) = test_db();
        let candle = Candle {
            symbol: "BTCUSDT".into(),
            timeframe: "5m".into(),
            open_time: 1000,
            open: 100.0,
            high: 110.0,
            low: 90.0,
            close: 105.0,
            volume: 500.0,
            source: "ws".into(),
        };

        db.with_writer(|conn| insert_candle(conn, &candle)).unwrap();

        // Upsert with updated close
        let updated = Candle {
            close: 108.0,
            source: "rest".into(),
            ..candle.clone()
        };
        db.with_writer(|conn| insert_candle(conn, &updated))
            .unwrap();

        let result = db
            .with_reader(|conn| query_candles(conn, "BTCUSDT", "5m", 0, 2000))
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].close, 108.0);
        assert_eq!(result[0].source, "rest");
    }

    #[test]
    fn candle_batch_insert() {
        let (_tmp, db) = test_db();
        let candles: Vec<Candle> = (0..100)
            .map(|i| Candle {
                symbol: "ETHUSDT".into(),
                timeframe: "5m".into(),
                open_time: i * 300_000,
                open: 3000.0 + i as f64,
                high: 3010.0 + i as f64,
                low: 2990.0 + i as f64,
                close: 3005.0 + i as f64,
                volume: 100.0,
                source: "rest".into(),
            })
            .collect();

        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let result = db
            .with_reader(|conn| query_candles(conn, "ETHUSDT", "5m", 0, i64::MAX))
            .unwrap();
        assert_eq!(result.len(), 100);
    }

    #[test]
    fn candle_range_query() {
        let (_tmp, db) = test_db();
        let candles: Vec<Candle> = (0..10)
            .map(|i| Candle {
                symbol: "BTCUSDT".into(),
                timeframe: "5m".into(),
                open_time: i * 300_000,
                open: 100.0,
                high: 110.0,
                low: 90.0,
                close: 105.0,
                volume: 50.0,
                source: "ws".into(),
            })
            .collect();

        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        // Query middle range: open_time 900_000..2100_000 should get indices 3,4,5,6
        let result = db
            .with_reader(|conn| query_candles(conn, "BTCUSDT", "5m", 900_000, 2_100_000))
            .unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].open_time, 900_000);
        assert_eq!(result[3].open_time, 1_800_000);
    }

    #[test]
    fn candle_latest_query() {
        let (_tmp, db) = test_db();
        let candles: Vec<Candle> = (0..5)
            .map(|i| Candle {
                symbol: "BTCUSDT".into(),
                timeframe: "1h".into(),
                open_time: i * 3_600_000,
                open: 100.0,
                high: 110.0,
                low: 90.0,
                close: 105.0,
                volume: 50.0,
                source: "ws".into(),
            })
            .collect();

        db.with_writer(|conn| insert_candles(conn, &candles))
            .unwrap();

        let latest = db
            .with_reader(|conn| query_latest_candle(conn, "BTCUSDT", "1h"))
            .unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().open_time, 4 * 3_600_000);

        // No data for different symbol
        let none = db
            .with_reader(|conn| query_latest_candle(conn, "ETHUSDT", "1h"))
            .unwrap();
        assert!(none.is_none());
    }

    #[test]
    fn funding_rate_roundtrip() {
        let (_tmp, db) = test_db();
        let rate = FundingRate {
            symbol: "BTCUSDT".into(),
            timestamp: 1000,
            rate: 0.0001,
        };

        db.with_writer(|conn| insert_funding_rate(conn, &rate))
            .unwrap();

        let result = db
            .with_reader(|conn| query_funding_rates(conn, "BTCUSDT", 0, 2000))
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], rate);
    }

    #[test]
    fn funding_rate_latest() {
        let (_tmp, db) = test_db();
        for i in 0..3 {
            let rate = FundingRate {
                symbol: "BTCUSDT".into(),
                timestamp: i * 28_800_000,
                rate: 0.0001 * (i + 1) as f64,
            };
            db.with_writer(|conn| insert_funding_rate(conn, &rate))
                .unwrap();
        }

        let latest = db
            .with_reader(|conn| query_latest_funding_rate(conn, "BTCUSDT"))
            .unwrap()
            .unwrap();
        assert_eq!(latest.timestamp, 2 * 28_800_000);
    }

    #[test]
    fn open_interest_roundtrip() {
        let (_tmp, db) = test_db();
        let oi = OpenInterest {
            symbol: "BTCUSDT".into(),
            timestamp: 1000,
            value: 5_000_000_000.0,
        };

        db.with_writer(|conn| insert_open_interest(conn, &oi))
            .unwrap();

        let result = db
            .with_reader(|conn| query_open_interest(conn, "BTCUSDT", 0, 2000))
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], oi);
    }

    #[test]
    fn open_interest_latest() {
        let (_tmp, db) = test_db();
        for i in 0..3 {
            let oi = OpenInterest {
                symbol: "ETHUSDT".into(),
                timestamp: i * 3_600_000,
                value: 1_000_000.0 * (i + 1) as f64,
            };
            db.with_writer(|conn| insert_open_interest(conn, &oi))
                .unwrap();
        }

        let latest = db
            .with_reader(|conn| query_latest_open_interest(conn, "ETHUSDT"))
            .unwrap()
            .unwrap();
        assert_eq!(latest.timestamp, 2 * 3_600_000);
    }

    #[test]
    fn index_value_roundtrip() {
        let (_tmp, db) = test_db();
        let idx = IndexValue {
            index_type: "fear_greed".into(),
            timestamp: 1000,
            value: 72.0,
        };

        db.with_writer(|conn| insert_index_value(conn, &idx))
            .unwrap();

        let result = db
            .with_reader(|conn| query_index_values(conn, "fear_greed", 0, 2000))
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], idx);
    }

    #[test]
    fn index_value_latest() {
        let (_tmp, db) = test_db();
        for i in 0..3 {
            let idx = IndexValue {
                index_type: "btc_dominance".into(),
                timestamp: i * 86_400_000,
                value: 55.0 + i as f64,
            };
            db.with_writer(|conn| insert_index_value(conn, &idx))
                .unwrap();
        }

        let latest = db
            .with_reader(|conn| query_latest_index_value(conn, "btc_dominance"))
            .unwrap()
            .unwrap();
        assert_eq!(latest.timestamp, 2 * 86_400_000);
        assert_eq!(latest.value, 57.0);
    }

    #[test]
    fn different_symbols_isolated() {
        let (_tmp, db) = test_db();
        let btc = Candle {
            symbol: "BTCUSDT".into(),
            timeframe: "5m".into(),
            open_time: 1000,
            open: 60000.0,
            high: 61000.0,
            low: 59000.0,
            close: 60500.0,
            volume: 100.0,
            source: "ws".into(),
        };
        let eth = Candle {
            symbol: "ETHUSDT".into(),
            timeframe: "5m".into(),
            open_time: 1000,
            open: 3000.0,
            high: 3100.0,
            low: 2900.0,
            close: 3050.0,
            volume: 200.0,
            source: "ws".into(),
        };

        db.with_writer(|conn| {
            insert_candle(conn, &btc)?;
            insert_candle(conn, &eth)
        })
        .unwrap();

        let btc_result = db
            .with_reader(|conn| query_candles(conn, "BTCUSDT", "5m", 0, 2000))
            .unwrap();
        assert_eq!(btc_result.len(), 1);
        assert_eq!(btc_result[0].symbol, "BTCUSDT");

        let eth_result = db
            .with_reader(|conn| query_candles(conn, "ETHUSDT", "5m", 0, 2000))
            .unwrap();
        assert_eq!(eth_result.len(), 1);
        assert_eq!(eth_result[0].symbol, "ETHUSDT");
    }
}
