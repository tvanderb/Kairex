pub mod backfill;
pub mod binance;
pub mod error;
pub mod event;
pub mod external;
pub mod polling;

pub use error::{CollectionError, Result};
pub use event::{CollectionEvent, DataType, EventSource};

use std::time::Duration;

use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::info;

use crate::config::{AssetsConfig, CollectionConfig};
use crate::storage::Database;

use backfill::BackfillOrchestrator;
use binance::{BinanceRestClient, BinanceWebSocket};
use external::{CoinGeckoClient, FearGreedClient};

/// Async-safe bridge: run a synchronous Database closure on a blocking thread.
pub async fn db_blocking<F, T>(db: &Database, f: F) -> Result<T>
where
    F: FnOnce(&Database) -> crate::storage::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || f(&db).map_err(CollectionError::Storage)).await?
}

/// Top-level collection layer orchestrator.
pub struct CollectionLayer {
    db: Database,
    assets_config: AssetsConfig,
    collection_config: CollectionConfig,
    event_tx: broadcast::Sender<CollectionEvent>,
}

impl CollectionLayer {
    pub fn new(
        db: Database,
        assets_config: AssetsConfig,
        collection_config: CollectionConfig,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
            assets_config,
            collection_config,
            event_tx,
        }
    }

    /// Subscribe to collection events.
    pub fn subscribe(&self) -> broadcast::Receiver<CollectionEvent> {
        self.event_tx.subscribe()
    }

    /// Start the collection layer:
    /// 1. Run initial backfill for all data types
    /// 2. Spawn the WebSocket listener
    /// 3. Spawn polling loops for funding rates, OI, fear & greed, dominance
    pub async fn start(&self) -> Result<Vec<JoinHandle<()>>> {
        let assets: Vec<String> = self
            .assets_config
            .symbols()
            .into_iter()
            .map(String::from)
            .collect();
        let timeframes = self.collection_config.websocket.timeframes.clone();

        let coingecko_key = std::env::var("COINGECKO_API_KEY").ok();

        let binance = BinanceRestClient::new();
        let fear_greed = FearGreedClient::new();
        let coingecko = CoinGeckoClient::new(coingecko_key.clone());

        // 1. Initial backfill
        let backfill = BackfillOrchestrator::new(
            BinanceRestClient::new(),
            FearGreedClient::new(),
            CoinGeckoClient::new(coingecko_key.clone()),
            self.db.clone(),
        );

        info!("starting initial backfill");
        match backfill.backfill_all(&assets, &timeframes).await {
            Ok(summary) => {
                info!(
                    candles = summary.candles_backfilled,
                    funding = summary.funding_rates_backfilled,
                    oi = summary.open_interest_backfilled,
                    indices = summary.indices_backfilled,
                    "initial backfill complete"
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "initial backfill failed, continuing with live collection");
            }
        }

        let mut handles = Vec::new();

        // 2. Spawn WebSocket
        let ws_backfill = BackfillOrchestrator::new(
            BinanceRestClient::new(),
            FearGreedClient::new(),
            CoinGeckoClient::new(coingecko_key),
            self.db.clone(),
        );

        let ws = BinanceWebSocket::new(
            assets.clone(),
            timeframes.clone(),
            self.collection_config.websocket.clone(),
            self.db.clone(),
            self.event_tx.clone(),
            ws_backfill,
        );

        handles.push(tokio::spawn(async move {
            ws.run().await;
        }));

        let retry = self.collection_config.retry.clone();

        // 3. Spawn polling loops

        // Funding rates
        let funding_interval = Duration::from_secs(
            self.collection_config
                .polling
                .funding_rates
                .interval_minutes
                * 60,
        );
        handles.push(polling::spawn_funding_rate_poll(
            assets.clone(),
            funding_interval,
            retry.clone(),
            binance,
            self.db.clone(),
            self.event_tx.clone(),
        ));

        // Open interest
        let oi_interval = Duration::from_secs(
            self.collection_config
                .polling
                .open_interest
                .interval_minutes
                * 60,
        );
        handles.push(polling::spawn_open_interest_poll(
            assets.clone(),
            oi_interval,
            retry.clone(),
            BinanceRestClient::new(),
            self.db.clone(),
            self.event_tx.clone(),
        ));

        // Fear & Greed
        let fg_interval =
            Duration::from_secs(self.collection_config.polling.fear_greed.interval_minutes * 60);
        handles.push(polling::spawn_fear_greed_poll(
            fg_interval,
            retry.clone(),
            fear_greed,
            self.db.clone(),
            self.event_tx.clone(),
        ));

        // Dominance
        let dom_interval =
            Duration::from_secs(self.collection_config.polling.dominance.interval_minutes * 60);
        handles.push(polling::spawn_dominance_poll(
            dom_interval,
            retry,
            coingecko,
            self.db.clone(),
            self.event_tx.clone(),
        ));

        info!(
            assets = assets.len(),
            timeframes = ?timeframes,
            "collection layer started"
        );

        Ok(handles)
    }
}
