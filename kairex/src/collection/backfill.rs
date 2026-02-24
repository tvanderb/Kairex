use tracing::{debug, error, info, instrument};

use crate::collection::binance::BinanceRestClient;
use crate::collection::error::Result;
use crate::collection::external::{CoinGeckoClient, FearGreedClient};
use crate::storage::Database;

// -- Gap detection (pure functions) --

#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub start_time: i64,
    pub end_time: i64,
}

/// Detect a gap in candle data.
///
/// - Cold start (no data): gap from `now - max_lookback_ms` to `now`
/// - Gap exists: gap from `latest_open_time + interval_ms` to `now`
/// - Within one interval: no gap (None)
pub fn detect_candle_gap(
    latest_open_time: Option<i64>,
    now: i64,
    interval_ms: i64,
    max_lookback_ms: i64,
) -> Option<Gap> {
    match latest_open_time {
        None => {
            // Cold start: backfill from max lookback
            Some(Gap {
                start_time: now - max_lookback_ms,
                end_time: now,
            })
        }
        Some(latest) => {
            let expected_next = latest + interval_ms;
            if expected_next < now {
                Some(Gap {
                    start_time: expected_next,
                    end_time: now,
                })
            } else {
                None
            }
        }
    }
}

/// Detect a gap in funding rate data. Funding rates come every 8h (28_800_000 ms).
pub fn detect_funding_gap(latest_timestamp: Option<i64>, now: i64) -> Option<Gap> {
    const FUNDING_INTERVAL_MS: i64 = 28_800_000; // 8 hours
    const MAX_LOOKBACK_MS: i64 = 30 * 24 * 3600 * 1000; // 30 days

    match latest_timestamp {
        None => Some(Gap {
            start_time: now - MAX_LOOKBACK_MS,
            end_time: now,
        }),
        Some(latest) => {
            if latest + FUNDING_INTERVAL_MS < now {
                Some(Gap {
                    start_time: latest + 1,
                    end_time: now,
                })
            } else {
                None
            }
        }
    }
}

/// Detect a gap in open interest data. OI is polled every 1h.
pub fn detect_oi_gap(latest_timestamp: Option<i64>, now: i64) -> Option<Gap> {
    const OI_INTERVAL_MS: i64 = 3_600_000; // 1 hour

    match latest_timestamp {
        None => Some(Gap {
            start_time: now,
            end_time: now,
        }),
        Some(latest) => {
            if latest + OI_INTERVAL_MS < now {
                Some(Gap {
                    start_time: latest + 1,
                    end_time: now,
                })
            } else {
                None
            }
        }
    }
}

/// Detect a gap in index data (fear_greed, dominance). Daily polling.
pub fn detect_index_gap(latest_timestamp: Option<i64>, now: i64) -> Option<Gap> {
    const INDEX_INTERVAL_MS: i64 = 86_400_000; // 24 hours

    match latest_timestamp {
        None => Some(Gap {
            start_time: now,
            end_time: now,
        }),
        Some(latest) => {
            if latest + INDEX_INTERVAL_MS < now {
                Some(Gap {
                    start_time: latest + 1,
                    end_time: now,
                })
            } else {
                None
            }
        }
    }
}

// -- Backfill orchestrator --

/// Maps timeframe strings to their interval and max lookback in milliseconds.
fn timeframe_params(tf: &str) -> (i64, i64) {
    match tf {
        "5m" => (300_000, 30_i64 * 24 * 3600 * 1000), // 30 days
        "1h" => (3_600_000, 365_i64 * 24 * 3600 * 1000), // 1 year
        "1d" => (86_400_000, 7_i64 * 365 * 24 * 3600 * 1000), // 7 years
        _ => (300_000, 30_i64 * 24 * 3600 * 1000),    // default to 5m
    }
}

#[derive(Debug, Default)]
pub struct BackfillSummary {
    pub candles_backfilled: u64,
    pub funding_rates_backfilled: u64,
    pub open_interest_backfilled: u64,
    pub indices_backfilled: u64,
}

pub struct BackfillOrchestrator {
    binance: BinanceRestClient,
    fear_greed: FearGreedClient,
    coingecko: CoinGeckoClient,
    db: Database,
}

impl BackfillOrchestrator {
    pub fn new(
        binance: BinanceRestClient,
        fear_greed: FearGreedClient,
        coingecko: CoinGeckoClient,
        db: Database,
    ) -> Self {
        Self {
            binance,
            fear_greed,
            coingecko,
            db,
        }
    }

    /// Backfill candle data for all assets and timeframes.
    pub async fn backfill_candles(
        &self,
        assets: &[String],
        timeframes: &[String],
    ) -> Result<BackfillSummary> {
        let mut summary = BackfillSummary::default();
        let now = current_time_ms();

        for symbol in assets {
            for tf in timeframes {
                let (interval_ms, max_lookback_ms) = timeframe_params(tf);

                let latest = {
                    let db = self.db.clone();
                    let sym = symbol.clone();
                    let timeframe = tf.clone();
                    super::db_blocking(&db, move |db| db.query_latest_candle(&sym, &timeframe))
                        .await?
                };

                let latest_time = latest.map(|c| c.open_time);
                let gap = detect_candle_gap(latest_time, now, interval_ms, max_lookback_ms);

                if let Some(gap) = gap {
                    debug!(
                        symbol = %symbol,
                        timeframe = %tf,
                        gap_start = gap.start_time,
                        gap_end = gap.end_time,
                        "backfilling candle gap"
                    );

                    match self
                        .binance
                        .fetch_klines_range(symbol, tf, gap.start_time, gap.end_time)
                        .await
                    {
                        Ok(candles) => {
                            let count = candles.len() as u64;
                            if !candles.is_empty() {
                                let db = self.db.clone();
                                super::db_blocking(&db, move |db| db.insert_candles(&candles))
                                    .await?;
                            }
                            summary.candles_backfilled += count;
                            info!(
                                symbol = %symbol,
                                timeframe = %tf,
                                count,
                                "backfilled candles"
                            );
                        }
                        Err(e) => {
                            error!(
                                symbol = %symbol,
                                timeframe = %tf,
                                error = %e,
                                "failed to backfill candles"
                            );
                        }
                    }
                }
            }
        }

        Ok(summary)
    }

    /// Backfill funding rate data for all assets.
    pub async fn backfill_funding_rates(&self, assets: &[String]) -> Result<BackfillSummary> {
        let mut summary = BackfillSummary::default();
        let now = current_time_ms();

        for symbol in assets {
            let latest = {
                let db = self.db.clone();
                let sym = symbol.clone();
                super::db_blocking(&db, move |db| db.query_latest_funding_rate(&sym)).await?
            };

            let latest_time = latest.map(|r| r.timestamp);
            let gap = detect_funding_gap(latest_time, now);

            if let Some(gap) = gap {
                debug!(
                    symbol = %symbol,
                    gap_start = gap.start_time,
                    gap_end = gap.end_time,
                    "backfilling funding rate gap"
                );

                match self
                    .binance
                    .fetch_funding_rates(symbol, Some(gap.start_time), Some(gap.end_time), None)
                    .await
                {
                    Ok(rates) => {
                        let count = rates.len() as u64;
                        for rate in &rates {
                            let db = self.db.clone();
                            let rate = rate.clone();
                            super::db_blocking(&db, move |db| db.insert_funding_rate(&rate))
                                .await?;
                        }
                        summary.funding_rates_backfilled += count;
                        info!(symbol = %symbol, count, "backfilled funding rates");
                    }
                    Err(e) => {
                        error!(
                            symbol = %symbol,
                            error = %e,
                            "failed to backfill funding rates"
                        );
                    }
                }
            }
        }

        Ok(summary)
    }

    /// Backfill open interest for all assets (current snapshot only).
    pub async fn backfill_open_interest(&self, assets: &[String]) -> Result<BackfillSummary> {
        let mut summary = BackfillSummary::default();
        let now = current_time_ms();

        for symbol in assets {
            let latest = {
                let db = self.db.clone();
                let sym = symbol.clone();
                super::db_blocking(&db, move |db| db.query_latest_open_interest(&sym)).await?
            };

            let latest_time = latest.map(|o| o.timestamp);
            let gap = detect_oi_gap(latest_time, now);

            if gap.is_some() {
                match self.binance.fetch_open_interest(symbol).await {
                    Ok(oi) => {
                        let db = self.db.clone();
                        super::db_blocking(&db, move |db| db.insert_open_interest(&oi)).await?;
                        summary.open_interest_backfilled += 1;
                        info!(symbol = %symbol, "backfilled open interest");
                    }
                    Err(e) => {
                        error!(
                            symbol = %symbol,
                            error = %e,
                            "failed to backfill open interest"
                        );
                    }
                }
            }
        }

        Ok(summary)
    }

    /// Backfill index values (fear & greed, dominance).
    pub async fn backfill_indices(&self) -> Result<BackfillSummary> {
        let mut summary = BackfillSummary::default();
        let now = current_time_ms();

        // Fear & Greed
        {
            let latest = {
                let db = self.db.clone();
                super::db_blocking(&db, move |db| db.query_latest_index_value("fear_greed")).await?
            };

            let gap = detect_index_gap(latest.map(|v| v.timestamp), now);
            if gap.is_some() {
                match self.fear_greed.fetch_current().await {
                    Ok(value) => {
                        let db = self.db.clone();
                        super::db_blocking(&db, move |db| db.insert_index_value(&value)).await?;
                        summary.indices_backfilled += 1;
                        info!("backfilled Fear & Greed index");
                    }
                    Err(e) => {
                        error!(error = %e, "failed to backfill Fear & Greed");
                    }
                }
            }
        }

        // CoinGecko global
        {
            let latest = {
                let db = self.db.clone();
                super::db_blocking(&db, move |db| db.query_latest_index_value("btc_dominance"))
                    .await?
            };

            let gap = detect_index_gap(latest.map(|v| v.timestamp), now);
            if gap.is_some() {
                match self.coingecko.fetch_global().await {
                    Ok(values) => {
                        let count = values.len() as u64;
                        for value in &values {
                            let db = self.db.clone();
                            let v = value.clone();
                            super::db_blocking(&db, move |db| db.insert_index_value(&v)).await?;
                        }
                        summary.indices_backfilled += count;
                        info!(count, "backfilled CoinGecko indices");
                    }
                    Err(e) => {
                        error!(error = %e, "failed to backfill CoinGecko data");
                    }
                }
            }
        }

        Ok(summary)
    }

    /// Run all backfill operations.
    #[instrument(name = "collection.backfill", skip_all)]
    pub async fn backfill_all(
        &self,
        assets: &[String],
        timeframes: &[String],
    ) -> Result<BackfillSummary> {
        let mut summary = BackfillSummary::default();

        let candle_summary = self.backfill_candles(assets, timeframes).await?;
        summary.candles_backfilled += candle_summary.candles_backfilled;

        let funding_summary = self.backfill_funding_rates(assets).await?;
        summary.funding_rates_backfilled += funding_summary.funding_rates_backfilled;

        let oi_summary = self.backfill_open_interest(assets).await?;
        summary.open_interest_backfilled += oi_summary.open_interest_backfilled;

        let index_summary = self.backfill_indices().await?;
        summary.indices_backfilled += index_summary.indices_backfilled;

        info!(
            candles = summary.candles_backfilled,
            funding = summary.funding_rates_backfilled,
            oi = summary.open_interest_backfilled,
            indices = summary.indices_backfilled,
            "backfill complete"
        );

        Ok(summary)
    }
}

fn current_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIVE_MIN_MS: i64 = 300_000;
    const HOUR_MS: i64 = 3_600_000;
    const DAY_30_MS: i64 = 30 * 24 * 3600 * 1000;

    #[test]
    fn cold_start_produces_max_lookback_gap() {
        let now = 1_000_000_000;
        let gap = detect_candle_gap(None, now, FIVE_MIN_MS, DAY_30_MS).unwrap();
        assert_eq!(gap.start_time, now - DAY_30_MS);
        assert_eq!(gap.end_time, now);
    }

    #[test]
    fn normal_gap_detected() {
        let now = 1_000_000_000;
        let latest = now - 600_000; // 2 intervals behind (5m)
        let gap = detect_candle_gap(Some(latest), now, FIVE_MIN_MS, DAY_30_MS).unwrap();
        assert_eq!(gap.start_time, latest + FIVE_MIN_MS);
        assert_eq!(gap.end_time, now);
    }

    #[test]
    fn within_interval_no_gap() {
        let now = 1_000_000_000;
        let latest = now - 100_000; // within 5m interval
        let gap = detect_candle_gap(Some(latest), now, FIVE_MIN_MS, DAY_30_MS);
        assert!(gap.is_none());
    }

    #[test]
    fn exactly_one_interval_no_gap() {
        let now = 1_000_000_000;
        let latest = now - FIVE_MIN_MS; // exactly one interval
        let gap = detect_candle_gap(Some(latest), now, FIVE_MIN_MS, DAY_30_MS);
        // latest + interval_ms == now, so expected_next is not < now
        assert!(gap.is_none());
    }

    #[test]
    fn funding_gap_cold_start() {
        let now = 1_000_000_000;
        let gap = detect_funding_gap(None, now).unwrap();
        assert_eq!(gap.start_time, now - DAY_30_MS);
        assert_eq!(gap.end_time, now);
    }

    #[test]
    fn funding_gap_detected() {
        let now = 1_000_000_000;
        let latest = now - 30_000_000; // more than 8h ago
        let gap = detect_funding_gap(Some(latest), now).unwrap();
        assert_eq!(gap.start_time, latest + 1);
        assert_eq!(gap.end_time, now);
    }

    #[test]
    fn funding_no_gap() {
        let now = 1_000_000_000;
        let latest = now - 1_000_000; // within 8h
        assert!(detect_funding_gap(Some(latest), now).is_none());
    }

    #[test]
    fn oi_gap_cold_start() {
        let now = 1_000_000_000;
        let gap = detect_oi_gap(None, now).unwrap();
        assert_eq!(gap.start_time, now);
    }

    #[test]
    fn oi_gap_detected() {
        let now = 1_000_000_000;
        let latest = now - HOUR_MS - 1;
        assert!(detect_oi_gap(Some(latest), now).is_some());
    }

    #[test]
    fn index_gap_cold_start() {
        let now = 1_000_000_000;
        let gap = detect_index_gap(None, now).unwrap();
        assert_eq!(gap.start_time, now);
    }

    #[test]
    fn index_no_gap_recent() {
        let now = 1_000_000_000;
        let latest = now - 1000; // very recent
        assert!(detect_index_gap(Some(latest), now).is_none());
    }
}
