use reqwest::Client;
use tracing::{debug, instrument, warn};

use crate::storage::{Candle, FundingRate, OpenInterest};

use super::convert::{binance_funding_to_model, binance_oi_to_model, kline_array_to_candle};
use super::types::{BinanceFundingRate, BinanceOpenInterest};
use crate::collection::error::{CollectionError, Result};

const SPOT_BASE: &str = "https://api.binance.com";
const FUTURES_BASE: &str = "https://fapi.binance.com";
const KLINE_LIMIT: u16 = 1000;
const WEIGHT_LIMIT: u64 = 1200;
const WEIGHT_THRESHOLD_PCT: u64 = 80;

pub struct BinanceRestClient {
    http: Client,
    spot_base: String,
    futures_base: String,
}

impl Default for BinanceRestClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BinanceRestClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            spot_base: SPOT_BASE.to_string(),
            futures_base: FUTURES_BASE.to_string(),
        }
    }

    /// Create a client with custom base URLs (for testing with wiremock).
    pub fn with_base_urls(spot_base: String, futures_base: String) -> Self {
        Self {
            http: Client::new(),
            spot_base,
            futures_base,
        }
    }

    /// Fetch klines for a single page.
    #[instrument(name = "collection.binance.fetch_klines", skip(self), fields(symbol = %symbol, interval = %interval))]
    pub async fn fetch_klines(
        &self,
        symbol: &str,
        interval: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: Option<u16>,
    ) -> Result<Vec<Candle>> {
        let mut url = format!("{}/api/v3/klines", self.spot_base);
        url.push_str(&format!("?symbol={symbol}&interval={interval}"));

        if let Some(start) = start_time {
            url.push_str(&format!("&startTime={start}"));
        }
        if let Some(end) = end_time {
            url.push_str(&format!("&endTime={end}"));
        }
        let lim = limit.unwrap_or(KLINE_LIMIT);
        url.push_str(&format!("&limit={lim}"));

        let response = self.http.get(&url).send().await?;
        self.check_rate_limit(&response);
        let response = response.error_for_status()?;

        let arrays: Vec<Vec<serde_json::Value>> = response.json().await?;

        let candles: Vec<Candle> = arrays
            .iter()
            .filter_map(|arr| kline_array_to_candle(arr, symbol, interval))
            .collect();

        debug!(
            symbol,
            interval,
            count = candles.len(),
            "fetched klines page"
        );
        Ok(candles)
    }

    /// Fetch klines across a time range, auto-paginating through 1000-candle pages.
    #[instrument(name = "collection.binance.fetch_klines_range", skip(self), fields(symbol = %symbol, interval = %interval))]
    pub async fn fetch_klines_range(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<Candle>> {
        let mut all_candles = Vec::new();
        let mut current_start = start_time;

        loop {
            let page = self
                .fetch_klines(
                    symbol,
                    interval,
                    Some(current_start),
                    Some(end_time),
                    Some(KLINE_LIMIT),
                )
                .await?;

            let page_len = page.len();
            if page_len == 0 {
                break;
            }

            let last_open_time = page.last().unwrap().open_time;
            all_candles.extend(page);

            // If we got fewer than the limit, we've exhausted the range
            if page_len < KLINE_LIMIT as usize {
                break;
            }

            // Advance past the last candle we received
            current_start = last_open_time + 1;
        }

        debug!(
            symbol,
            interval,
            total = all_candles.len(),
            "fetched klines range"
        );
        Ok(all_candles)
    }

    /// Fetch funding rate history for a symbol.
    #[instrument(name = "collection.binance.fetch_funding_rates", skip(self), fields(symbol = %symbol))]
    pub async fn fetch_funding_rates(
        &self,
        symbol: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: Option<u16>,
    ) -> Result<Vec<FundingRate>> {
        let mut url = format!("{}/fapi/v1/fundingRate?symbol={symbol}", self.futures_base);

        if let Some(start) = start_time {
            url.push_str(&format!("&startTime={start}"));
        }
        if let Some(end) = end_time {
            url.push_str(&format!("&endTime={end}"));
        }
        if let Some(lim) = limit {
            url.push_str(&format!("&limit={lim}"));
        }

        let response = self.http.get(&url).send().await?;
        self.check_rate_limit(&response);
        let response = response.error_for_status()?;

        let raw: Vec<BinanceFundingRate> = response.json().await?;
        let rates: Vec<FundingRate> = raw.iter().filter_map(binance_funding_to_model).collect();

        debug!(symbol, count = rates.len(), "fetched funding rates");
        Ok(rates)
    }

    /// Fetch current open interest for a symbol.
    #[instrument(name = "collection.binance.fetch_open_interest", skip(self), fields(symbol = %symbol))]
    pub async fn fetch_open_interest(&self, symbol: &str) -> Result<OpenInterest> {
        let url = format!("{}/fapi/v1/openInterest?symbol={symbol}", self.futures_base);

        let response = self.http.get(&url).send().await?;
        self.check_rate_limit(&response);
        let response = response.error_for_status()?;

        let raw: BinanceOpenInterest = response.json().await?;
        binance_oi_to_model(&raw).ok_or_else(|| CollectionError::Api {
            message: format!("failed to parse open interest for {symbol}"),
        })
    }

    /// Check rate limit headers and log warnings if approaching limit.
    fn check_rate_limit(&self, response: &reqwest::Response) {
        if let Some(weight) = response
            .headers()
            .get("X-MBX-USED-WEIGHT-1M")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            let threshold = WEIGHT_LIMIT * WEIGHT_THRESHOLD_PCT / 100;
            if weight > threshold {
                warn!(
                    weight,
                    limit = WEIGHT_LIMIT,
                    "approaching Binance rate limit"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_kline_array() -> serde_json::Value {
        serde_json::json!([
            1708992000000_i64,
            "51234.56",
            "51500.00",
            "51000.00",
            "51300.00",
            "1234.567",
            1708992299999_i64,
            "63234567.89",
            5432,
            "617.283",
            "31617283.94",
            "0"
        ])
    }

    #[tokio::test]
    async fn fetch_klines_parses_response() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v3/klines"))
            .and(query_param("symbol", "BTCUSDT"))
            .and(query_param("interval", "5m"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_kline_array(),
                sample_kline_array(),
            ])))
            .mount(&server)
            .await;

        let client =
            BinanceRestClient::with_base_urls(server.uri(), format!("{}/futures", server.uri()));
        let candles = client
            .fetch_klines("BTCUSDT", "5m", None, None, Some(2))
            .await
            .unwrap();

        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].symbol, "BTCUSDT");
        assert_eq!(candles[0].timeframe, "5m");
        assert_eq!(candles[0].source, "rest");
    }

    #[tokio::test]
    async fn fetch_klines_range_single_page() {
        let server = MockServer::start().await;

        // Fewer than limit — stops after one call
        let kline1 = serde_json::json!([
            1000_i64, "100.0", "110.0", "90.0", "105.0", "50.0", 1299_i64, "5000.0", 100, "25.0",
            "2500.0", "0"
        ]);
        let kline2 = serde_json::json!([
            2000_i64, "105.0", "115.0", "95.0", "110.0", "60.0", 2299_i64, "6000.0", 120, "30.0",
            "3000.0", "0"
        ]);

        Mock::given(method("GET"))
            .and(path("/api/v3/klines"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([kline1, kline2])),
            )
            .expect(1) // exactly one call since results < limit
            .mount(&server)
            .await;

        let client =
            BinanceRestClient::with_base_urls(server.uri(), format!("{}/futures", server.uri()));
        let candles = client
            .fetch_klines_range("BTCUSDT", "5m", 1000, 5000)
            .await
            .unwrap();

        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].open_time, 1000);
        assert_eq!(candles[1].open_time, 2000);
    }

    #[tokio::test]
    async fn fetch_klines_range_paginates() {
        let server = MockServer::start().await;

        // Return exactly KLINE_LIMIT candles on first call to trigger pagination
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("GET"))
            .and(path("/api/v3/klines"))
            .respond_with(move |_req: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count == 0 {
                    // First page: exactly 1000 candles — triggers pagination
                    let klines: Vec<serde_json::Value> = (0..1000)
                        .map(|i| {
                            let t = 1000_i64 + i * 300_000;
                            serde_json::json!([
                                t,
                                "100.0",
                                "110.0",
                                "90.0",
                                "105.0",
                                "50.0",
                                t + 299_999,
                                "5000.0",
                                100,
                                "25.0",
                                "2500.0",
                                "0"
                            ])
                        })
                        .collect();
                    ResponseTemplate::new(200).set_body_json(serde_json::json!(klines))
                } else {
                    // Second page: empty — done
                    ResponseTemplate::new(200).set_body_json(serde_json::json!([]))
                }
            })
            .mount(&server)
            .await;

        let client =
            BinanceRestClient::with_base_urls(server.uri(), format!("{}/futures", server.uri()));
        let candles = client
            .fetch_klines_range("BTCUSDT", "5m", 1000, i64::MAX)
            .await
            .unwrap();

        assert_eq!(candles.len(), 1000);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fetch_funding_rates_parses() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fapi/v1/fundingRate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "symbol": "BTCUSDT",
                    "fundingTime": 1708992000000_i64,
                    "fundingRate": "0.0001",
                    "markPrice": "51234.56"
                }
            ])))
            .mount(&server)
            .await;

        let client =
            BinanceRestClient::with_base_urls(format!("{}/spot", server.uri()), server.uri());
        let rates = client
            .fetch_funding_rates("BTCUSDT", None, None, None)
            .await
            .unwrap();

        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].symbol, "BTCUSDT");
        assert_eq!(rates[0].timestamp, 1708992000000);
        assert!((rates[0].rate - 0.0001).abs() < 1e-10);
    }

    #[tokio::test]
    async fn fetch_open_interest_parses() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fapi/v1/openInterest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "symbol": "BTCUSDT",
                "openInterest": "12345.678",
                "time": 1708992000000_i64
            })))
            .mount(&server)
            .await;

        let client =
            BinanceRestClient::with_base_urls(format!("{}/spot", server.uri()), server.uri());
        let oi = client.fetch_open_interest("BTCUSDT").await.unwrap();

        assert_eq!(oi.symbol, "BTCUSDT");
        assert_eq!(oi.timestamp, 1708992000000);
        assert!((oi.value - 12345.678).abs() < 0.001);
    }
}
