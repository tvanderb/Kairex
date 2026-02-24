use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument, warn};

use crate::collection::backfill::BackfillOrchestrator;
use crate::collection::event::CollectionEvent;
use crate::config::WebSocketConfig;
use crate::storage::Database;

pub struct BinanceWebSocket {
    assets: Vec<String>,
    timeframes: Vec<String>,
    ws_config: WebSocketConfig,
    db: Database,
    event_tx: broadcast::Sender<CollectionEvent>,
    backfill: BackfillOrchestrator,
}

impl BinanceWebSocket {
    pub fn new(
        assets: Vec<String>,
        timeframes: Vec<String>,
        ws_config: WebSocketConfig,
        db: Database,
        event_tx: broadcast::Sender<CollectionEvent>,
        backfill: BackfillOrchestrator,
    ) -> Self {
        Self {
            assets,
            timeframes,
            ws_config,
            db,
            event_tx,
            backfill,
        }
    }

    pub fn build_stream_url(&self) -> String {
        let streams: Vec<String> = self
            .assets
            .iter()
            .flat_map(|asset| {
                let lower = asset.to_lowercase();
                self.timeframes
                    .iter()
                    .map(move |tf| format!("{lower}@kline_{tf}"))
            })
            .collect();

        format!(
            "wss://stream.binance.com:9443/stream?streams={}",
            streams.join("/")
        )
    }

    /// Main WebSocket loop: connect, listen, backfill on disconnect, reconnect with backoff.
    #[instrument(name = "collection.websocket", skip_all)]
    pub async fn run(&self) {
        use crate::collection::binance::convert::ws_kline_to_candle;
        use crate::collection::binance::types::CombinedStreamMessage;
        use crate::collection::event::{DataType, EventSource};
        use futures_util::StreamExt;
        use std::time::Instant;

        let mut delay_ms = self.ws_config.reconnect_delay_ms;

        loop {
            let url = self.build_stream_url();
            info!(url = %url, "connecting to Binance WebSocket");

            let connect_result = tokio_tungstenite::connect_async(&url).await;

            match connect_result {
                Ok((ws_stream, _response)) => {
                    info!("WebSocket connected");
                    let connected_at = Instant::now();

                    let (_write, mut read) = ws_stream.split();

                    loop {
                        match read.next().await {
                            Some(Ok(msg)) => {
                                if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                                    match serde_json::from_str::<CombinedStreamMessage>(&text) {
                                        Ok(combined) => {
                                            let kline = &combined.data.kline;
                                            if !kline.is_final {
                                                continue;
                                            }

                                            if let Some(candle) = ws_kline_to_candle(kline) {
                                                let db = self.db.clone();
                                                let candle_clone = candle.clone();
                                                let store_result = crate::collection::db_blocking(
                                                    &db,
                                                    move |db| db.insert_candle(&candle_clone),
                                                )
                                                .await;

                                                match store_result {
                                                    Ok(()) => {
                                                        debug!(
                                                            symbol = %candle.symbol,
                                                            timeframe = %candle.timeframe,
                                                            open_time = candle.open_time,
                                                            "stored ws candle"
                                                        );
                                                        let _ =
                                                            self.event_tx.send(CollectionEvent {
                                                                source:
                                                                    EventSource::BinanceWebSocket,
                                                                symbol: Some(candle.symbol.clone()),
                                                                data_type: DataType::Candle {
                                                                    timeframe: candle
                                                                        .timeframe
                                                                        .clone(),
                                                                },
                                                                timestamp: candle.open_time,
                                                            });
                                                    }
                                                    Err(e) => {
                                                        error!(
                                                            error = %e,
                                                            symbol = %candle.symbol,
                                                            "failed to store ws candle"
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "failed to parse ws message");
                                        }
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                warn!(error = %e, "WebSocket read error");
                                break;
                            }
                            None => {
                                info!("WebSocket stream ended");
                                break;
                            }
                        }
                    }

                    // Connection dropped — backfill any gaps
                    info!("running gap backfill after disconnect");
                    if let Err(e) = self
                        .backfill
                        .backfill_candles(&self.assets, &self.timeframes)
                        .await
                    {
                        error!(error = %e, "backfill after disconnect failed");
                    }

                    // Reset backoff if connection was stable (>60s)
                    if connected_at.elapsed().as_secs() > 60 {
                        delay_ms = self.ws_config.reconnect_delay_ms;
                    }
                }
                Err(e) => {
                    error!(error = %e, "WebSocket connection failed");
                }
            }

            // Backoff before reconnecting
            warn!(delay_ms, "reconnecting after delay");
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            delay_ms = (delay_ms as f64 * self.ws_config.reconnect_backoff_multiplier) as u64;
            delay_ms = delay_ms.min(self.ws_config.reconnect_max_delay_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WebSocketConfig;

    fn test_ws_config() -> WebSocketConfig {
        WebSocketConfig {
            timeframes: vec!["5m".into(), "1h".into(), "1d".into()],
            reconnect_delay_ms: 1000,
            reconnect_max_delay_ms: 30000,
            reconnect_backoff_multiplier: 2.0,
        }
    }

    #[test]
    fn build_stream_url_correct() {
        let db = {
            let tmp = tempfile::tempdir().unwrap();
            Database::open_in_memory(tmp.path()).unwrap()
        };
        let (tx, _rx) = broadcast::channel(16);
        let backfill = BackfillOrchestrator::new(
            crate::collection::binance::BinanceRestClient::new(),
            crate::collection::external::FearGreedClient::new(),
            crate::collection::external::CoinGeckoClient::new(None),
            db.clone(),
        );
        let ws = BinanceWebSocket::new(
            vec!["BTCUSDT".into(), "ETHUSDT".into()],
            vec!["5m".into(), "1h".into()],
            test_ws_config(),
            db,
            tx,
            backfill,
        );

        let url = ws.build_stream_url();
        assert!(url.starts_with("wss://stream.binance.com:9443/stream?streams="));
        assert!(url.contains("btcusdt@kline_5m"));
        assert!(url.contains("btcusdt@kline_1h"));
        assert!(url.contains("ethusdt@kline_5m"));
        assert!(url.contains("ethusdt@kline_1h"));
    }

    #[test]
    fn ws_kline_final_filter() {
        use crate::collection::binance::convert::ws_kline_to_candle;
        use crate::collection::binance::types::KlineData;

        let final_kline = KlineData {
            open_time: 1000,
            close_time: 1299,
            symbol: "BTCUSDT".into(),
            interval: "5m".into(),
            open: "100.0".into(),
            high: "110.0".into(),
            low: "90.0".into(),
            close: "105.0".into(),
            volume: "50.0".into(),
            is_final: true,
        };

        let partial_kline = KlineData {
            is_final: false,
            ..final_kline.clone()
        };

        assert!(ws_kline_to_candle(&final_kline).is_some());
        assert!(ws_kline_to_candle(&partial_kline).is_some()); // conversion works for both
        assert!(final_kline.is_final);
        assert!(!partial_kline.is_final); // filtering is done by caller
    }

    #[test]
    fn ws_fixture_deserialize() {
        use crate::collection::binance::types::CombinedStreamMessage;

        let final_json = r#"{
            "stream": "btcusdt@kline_5m",
            "data": {
                "e": "kline",
                "E": 1708992300000,
                "s": "BTCUSDT",
                "k": {
                    "t": 1708992000000,
                    "T": 1708992299999,
                    "s": "BTCUSDT",
                    "i": "5m",
                    "o": "51234.56",
                    "h": "51500.00",
                    "l": "51000.00",
                    "c": "51300.00",
                    "v": "123.456",
                    "x": true
                }
            }
        }"#;

        let msg: CombinedStreamMessage = serde_json::from_str(final_json).unwrap();
        assert_eq!(msg.stream, "btcusdt@kline_5m");
        assert!(msg.data.kline.is_final);
        assert_eq!(msg.data.kline.open_time, 1708992000000);

        let partial_json = r#"{
            "stream": "btcusdt@kline_5m",
            "data": {
                "e": "kline",
                "E": 1708992100000,
                "s": "BTCUSDT",
                "k": {
                    "t": 1708992000000,
                    "T": 1708992299999,
                    "s": "BTCUSDT",
                    "i": "5m",
                    "o": "51234.56",
                    "h": "51400.00",
                    "l": "51100.00",
                    "c": "51350.00",
                    "v": "50.123",
                    "x": false
                }
            }
        }"#;

        let msg: CombinedStreamMessage = serde_json::from_str(partial_json).unwrap();
        assert!(!msg.data.kline.is_final);
    }
}
