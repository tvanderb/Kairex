use reqwest::Client;
use tracing::debug;

use crate::collection::error::{CollectionError, Result};
use crate::storage::IndexValue;

use super::types::FearGreedResponse;

const BASE_URL: &str = "https://api.alternative.me/fng/";

pub struct FearGreedClient {
    http: Client,
    base_url: String,
}

impl Default for FearGreedClient {
    fn default() -> Self {
        Self::new()
    }
}

impl FearGreedClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            base_url: BASE_URL.to_string(),
        }
    }

    /// Create a client with a custom base URL (for testing).
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: Client::new(),
            base_url,
        }
    }

    /// Fetch the current Fear & Greed index value.
    pub async fn fetch_current(&self) -> Result<IndexValue> {
        let response = self.http.get(&self.base_url).send().await?;
        let response = response.error_for_status()?;
        let data: FearGreedResponse = response.json().await?;

        let entry = data.data.first().ok_or_else(|| CollectionError::Api {
            message: "empty Fear & Greed response".into(),
        })?;

        let value: f64 = entry.value.parse().map_err(|_| CollectionError::Api {
            message: format!("invalid Fear & Greed value: {}", entry.value),
        })?;

        // Alternative.me returns timestamp in seconds — convert to milliseconds
        let timestamp_secs: i64 = entry.timestamp.parse().map_err(|_| CollectionError::Api {
            message: format!("invalid Fear & Greed timestamp: {}", entry.timestamp),
        })?;
        let timestamp = timestamp_secs * 1000;

        debug!(value, timestamp, "fetched Fear & Greed index");

        Ok(IndexValue {
            index_type: "fear_greed".to_string(),
            timestamp,
            value,
        })
    }

    /// Fetch historical Fear & Greed values.
    pub async fn fetch_history(&self, days: u32) -> Result<Vec<IndexValue>> {
        let url = format!("{}?limit={}", self.base_url, days);
        let response = self.http.get(&url).send().await?;
        let response = response.error_for_status()?;
        let data: FearGreedResponse = response.json().await?;

        let values: Vec<IndexValue> = data
            .data
            .iter()
            .filter_map(|entry| {
                let value: f64 = entry.value.parse().ok()?;
                let timestamp_secs: i64 = entry.timestamp.parse().ok()?;
                Some(IndexValue {
                    index_type: "fear_greed".to_string(),
                    timestamp: timestamp_secs * 1000,
                    value,
                })
            })
            .collect();

        debug!(count = values.len(), "fetched Fear & Greed history");
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_current_parses_and_converts_timestamp() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "value": "72",
                    "value_classification": "Greed",
                    "timestamp": "1708992000"
                }]
            })))
            .mount(&server)
            .await;

        let client = FearGreedClient::with_base_url(format!("{}/", server.uri()));
        let result = client.fetch_current().await.unwrap();

        assert_eq!(result.index_type, "fear_greed");
        assert!((result.value - 72.0).abs() < 0.01);
        // Timestamp should be in milliseconds (seconds * 1000)
        assert_eq!(result.timestamp, 1708992000 * 1000);
    }

    #[tokio::test]
    async fn fetch_history_returns_multiple() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"value": "72", "value_classification": "Greed", "timestamp": "1708992000"},
                    {"value": "65", "value_classification": "Greed", "timestamp": "1708905600"},
                    {"value": "58", "value_classification": "Greed", "timestamp": "1708819200"}
                ]
            })))
            .mount(&server)
            .await;

        let client = FearGreedClient::with_base_url(format!("{}/", server.uri()));
        let results = client.fetch_history(3).await.unwrap();

        assert_eq!(results.len(), 3);
        assert!((results[0].value - 72.0).abs() < 0.01);
        assert!((results[2].value - 58.0).abs() < 0.01);
    }

    #[test]
    fn seconds_to_ms_conversion() {
        // Verify we're converting correctly: 1708992000 seconds -> 1708992000000 ms
        let secs: i64 = 1708992000;
        let ms = secs * 1000;
        assert_eq!(ms, 1708992000000);
    }
}
