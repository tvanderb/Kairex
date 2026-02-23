use std::path::Path;

use serde::Deserialize;

use super::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct CollectionConfig {
    pub websocket: WebSocketConfig,
    pub polling: PollingConfig,
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSocketConfig {
    pub timeframes: Vec<String>,
    pub reconnect_delay_ms: u64,
    pub reconnect_max_delay_ms: u64,
    pub reconnect_backoff_multiplier: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PollingConfig {
    pub funding_rates: PollEndpoint,
    pub open_interest: PollEndpoint,
    pub fear_greed: PollEndpoint,
    pub dominance: PollEndpoint,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PollEndpoint {
    pub interval_minutes: u64,
    pub source: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl CollectionConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tests/fixtures/config/collection.toml")
    }

    #[test]
    fn parse_collection_fixture() {
        let config = CollectionConfig::load(&fixture_path()).unwrap();

        // WebSocket
        assert_eq!(config.websocket.timeframes, vec!["5m", "1h", "1d"]);
        assert_eq!(config.websocket.reconnect_delay_ms, 1000);
        assert_eq!(config.websocket.reconnect_max_delay_ms, 30000);
        assert_eq!(config.websocket.reconnect_backoff_multiplier, 2.0);

        // Polling
        assert_eq!(config.polling.funding_rates.interval_minutes, 480);
        assert_eq!(config.polling.funding_rates.source, "binance_futures");
        assert_eq!(
            config.polling.funding_rates.endpoint.as_deref(),
            Some("/fapi/v1/fundingRate")
        );

        assert_eq!(config.polling.open_interest.interval_minutes, 60);
        assert_eq!(config.polling.fear_greed.interval_minutes, 1440);
        assert_eq!(config.polling.fear_greed.source, "alternative_me");
        assert!(config.polling.fear_greed.endpoint.is_none());

        assert_eq!(config.polling.dominance.interval_minutes, 60);
        assert_eq!(config.polling.dominance.source, "coingecko");

        // Retry
        assert_eq!(config.retry.max_attempts, 3);
        assert_eq!(config.retry.initial_delay_ms, 1000);
        assert_eq!(config.retry.backoff_multiplier, 2.0);
    }

    #[test]
    fn missing_file_returns_io_error() {
        let result = CollectionConfig::load(Path::new("/nonexistent/collection.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, super::super::ConfigError::Io(_)));
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "{{invalid}}").unwrap();
        let result = CollectionConfig::load(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, super::super::ConfigError::Parse(_)));
    }
}
