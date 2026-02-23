use std::time::Duration;

use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::collection::binance::BinanceRestClient;
use crate::collection::error::CollectionError;
use crate::collection::event::{CollectionEvent, DataType, EventSource};
use crate::collection::external::{CoinGeckoClient, FearGreedClient};
use crate::config::RetryConfig;
use crate::storage::Database;

const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Generic poll loop with retry and failure tracking.
pub struct PollLoop {
    name: String,
    interval: Duration,
    retry_config: RetryConfig,
}

impl PollLoop {
    pub fn new(name: &str, interval: Duration, retry_config: RetryConfig) -> Self {
        Self {
            name: name.to_string(),
            interval,
            retry_config,
        }
    }

    /// Run the poll loop forever, calling `task_fn` at each interval.
    pub async fn run<F, Fut>(&self, task_fn: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), CollectionError>>,
    {
        let mut consecutive_failures: u32 = 0;

        loop {
            tokio::time::sleep(self.interval).await;

            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                warn!(
                    name = %self.name,
                    failures = consecutive_failures,
                    "skipping poll iteration due to consecutive failures, resetting counter"
                );
                consecutive_failures = 0;
                continue;
            }

            match self.execute_with_retry(&task_fn).await {
                Ok(()) => {
                    consecutive_failures = 0;
                    debug!(name = %self.name, "poll iteration succeeded");
                }
                Err(e) => {
                    consecutive_failures += 1;
                    error!(
                        name = %self.name,
                        error = %e,
                        consecutive_failures,
                        "poll iteration failed"
                    );
                }
            }
        }
    }

    async fn execute_with_retry<F, Fut>(&self, task_fn: &F) -> Result<(), CollectionError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), CollectionError>>,
    {
        let mut delay_ms = self.retry_config.initial_delay_ms;

        for attempt in 1..=self.retry_config.max_attempts {
            match task_fn().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if attempt == self.retry_config.max_attempts {
                        return Err(e);
                    }
                    warn!(
                        name = %self.name,
                        attempt,
                        max = self.retry_config.max_attempts,
                        error = %e,
                        "retrying after failure"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms as f64 * self.retry_config.backoff_multiplier) as u64;
                }
            }
        }

        unreachable!()
    }
}

/// Spawn a funding rate polling task.
pub fn spawn_funding_rate_poll(
    assets: Vec<String>,
    interval: Duration,
    retry_config: RetryConfig,
    binance: BinanceRestClient,
    db: Database,
    event_tx: broadcast::Sender<CollectionEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let poll = PollLoop::new("funding_rates", interval, retry_config);
        poll.run(|| {
            let assets = assets.clone();
            let binance = &binance;
            let db = db.clone();
            let event_tx = event_tx.clone();
            async move {
                for symbol in &assets {
                    match binance
                        .fetch_funding_rates(symbol, None, None, Some(1))
                        .await
                    {
                        Ok(rates) => {
                            for rate in &rates {
                                let db = db.clone();
                                let rate = rate.clone();
                                let ts = rate.timestamp;
                                let sym = rate.symbol.clone();
                                super::db_blocking(&db, move |db| db.insert_funding_rate(&rate))
                                    .await?;
                                let _ = event_tx.send(CollectionEvent {
                                    source: EventSource::BinanceRest,
                                    symbol: Some(sym),
                                    data_type: DataType::FundingRate,
                                    timestamp: ts,
                                });
                            }
                        }
                        Err(e) => {
                            error!(symbol = %symbol, error = %e, "failed to fetch funding rate");
                            return Err(e);
                        }
                    }
                }
                info!("polled funding rates for all assets");
                Ok(())
            }
        })
        .await;
    })
}

/// Spawn an open interest polling task.
pub fn spawn_open_interest_poll(
    assets: Vec<String>,
    interval: Duration,
    retry_config: RetryConfig,
    binance: BinanceRestClient,
    db: Database,
    event_tx: broadcast::Sender<CollectionEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let poll = PollLoop::new("open_interest", interval, retry_config);
        poll.run(|| {
            let assets = assets.clone();
            let binance = &binance;
            let db = db.clone();
            let event_tx = event_tx.clone();
            async move {
                for symbol in &assets {
                    match binance.fetch_open_interest(symbol).await {
                        Ok(oi) => {
                            let ts = oi.timestamp;
                            let sym = oi.symbol.clone();
                            let db = db.clone();
                            super::db_blocking(&db, move |db| db.insert_open_interest(&oi)).await?;
                            let _ = event_tx.send(CollectionEvent {
                                source: EventSource::BinanceRest,
                                symbol: Some(sym),
                                data_type: DataType::OpenInterest,
                                timestamp: ts,
                            });
                        }
                        Err(e) => {
                            error!(symbol = %symbol, error = %e, "failed to fetch open interest");
                            return Err(e);
                        }
                    }
                }
                info!("polled open interest for all assets");
                Ok(())
            }
        })
        .await;
    })
}

/// Spawn a Fear & Greed index polling task.
pub fn spawn_fear_greed_poll(
    interval: Duration,
    retry_config: RetryConfig,
    fear_greed: FearGreedClient,
    db: Database,
    event_tx: broadcast::Sender<CollectionEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let poll = PollLoop::new("fear_greed", interval, retry_config);
        poll.run(|| {
            let fear_greed = &fear_greed;
            let db = db.clone();
            let event_tx = event_tx.clone();
            async move {
                let value = fear_greed.fetch_current().await?;
                let ts = value.timestamp;
                let db = db.clone();
                super::db_blocking(&db, move |db| db.insert_index_value(&value)).await?;
                let _ = event_tx.send(CollectionEvent {
                    source: EventSource::AlternativeMe,
                    symbol: None,
                    data_type: DataType::Index {
                        index_type: "fear_greed".into(),
                    },
                    timestamp: ts,
                });
                info!("polled Fear & Greed index");
                Ok(())
            }
        })
        .await;
    })
}

/// Spawn a CoinGecko dominance polling task.
pub fn spawn_dominance_poll(
    interval: Duration,
    retry_config: RetryConfig,
    coingecko: CoinGeckoClient,
    db: Database,
    event_tx: broadcast::Sender<CollectionEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let poll = PollLoop::new("dominance", interval, retry_config);
        poll.run(|| {
            let coingecko = &coingecko;
            let db = db.clone();
            let event_tx = event_tx.clone();
            async move {
                let values = coingecko.fetch_global().await?;
                for value in &values {
                    let db = db.clone();
                    let v = value.clone();
                    let ts = v.timestamp;
                    let idx_type = v.index_type.clone();
                    super::db_blocking(&db, move |db| db.insert_index_value(&v)).await?;
                    let _ = event_tx.send(CollectionEvent {
                        source: EventSource::CoinGecko,
                        symbol: None,
                        data_type: DataType::Index {
                            index_type: idx_type,
                        },
                        timestamp: ts,
                    });
                }
                info!("polled CoinGecko dominance data");
                Ok(())
            }
        })
        .await;
    })
}
