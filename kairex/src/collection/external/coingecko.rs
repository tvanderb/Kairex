use reqwest::Client;
use tracing::debug;

use crate::collection::error::{CollectionError, Result};
use crate::storage::IndexValue;

use super::types::CoinGeckoGlobalResponse;

const BASE_URL: &str = "https://api.coingecko.com/api/v3";

pub struct CoinGeckoClient {
    http: Client,
    base_url: String,
    api_key: Option<String>,
}

impl CoinGeckoClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: BASE_URL.to_string(),
            api_key,
        }
    }

    /// Create a client with a custom base URL (for testing).
    pub fn with_base_url(base_url: String, api_key: Option<String>) -> Self {
        Self {
            http: Client::new(),
            base_url,
            api_key,
        }
    }

    /// Fetch global market data: BTC dominance, ETH dominance, total market cap.
    /// Returns 3 IndexValues.
    pub async fn fetch_global(&self) -> Result<Vec<IndexValue>> {
        let url = format!("{}/global", self.base_url);

        let mut request = self.http.get(&url);
        if let Some(ref key) = self.api_key {
            request = request.header("x-cg-demo-api-key", key);
        }

        let response = request.send().await?;
        let response = response.error_for_status()?;
        let data: CoinGeckoGlobalResponse = response.json().await?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut values = Vec::with_capacity(3);

        if let Some(&btc_dom) = data.data.market_cap_percentage.get("btc") {
            values.push(IndexValue {
                index_type: "btc_dominance".to_string(),
                timestamp: now,
                value: btc_dom,
            });
        }

        if let Some(&eth_dom) = data.data.market_cap_percentage.get("eth") {
            values.push(IndexValue {
                index_type: "eth_dominance".to_string(),
                timestamp: now,
                value: eth_dom,
            });
        }

        if let Some(&total_mc) = data.data.total_market_cap.get("usd") {
            values.push(IndexValue {
                index_type: "total_market_cap".to_string(),
                timestamp: now,
                value: total_mc,
            });
        }

        if values.is_empty() {
            return Err(CollectionError::Api {
                message: "CoinGecko global response missing expected fields".into(),
            });
        }

        debug!(count = values.len(), "fetched CoinGecko global data");
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_global_returns_three_values() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/global"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "market_cap_percentage": {
                        "btc": 54.2,
                        "eth": 16.8,
                        "usdt": 4.5
                    },
                    "total_market_cap": {
                        "usd": 2_500_000_000_000.0,
                        "eur": 2_300_000_000_000.0
                    }
                }
            })))
            .mount(&server)
            .await;

        let client = CoinGeckoClient::with_base_url(server.uri(), None);
        let values = client.fetch_global().await.unwrap();

        assert_eq!(values.len(), 3);

        let btc = values
            .iter()
            .find(|v| v.index_type == "btc_dominance")
            .unwrap();
        assert!((btc.value - 54.2).abs() < 0.01);

        let eth = values
            .iter()
            .find(|v| v.index_type == "eth_dominance")
            .unwrap();
        assert!((eth.value - 16.8).abs() < 0.01);

        let mc = values
            .iter()
            .find(|v| v.index_type == "total_market_cap")
            .unwrap();
        assert!((mc.value - 2.5e12).abs() < 1e9);
    }

    #[tokio::test]
    async fn fetch_global_with_api_key() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/global"))
            .and(wiremock::matchers::header("x-cg-demo-api-key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "market_cap_percentage": {"btc": 54.0, "eth": 17.0},
                    "total_market_cap": {"usd": 2.5e12}
                }
            })))
            .mount(&server)
            .await;

        let client = CoinGeckoClient::with_base_url(server.uri(), Some("test-key".to_string()));
        let values = client.fetch_global().await.unwrap();
        assert_eq!(values.len(), 3);
    }
}
