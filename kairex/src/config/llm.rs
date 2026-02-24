use std::path::Path;

use serde::Deserialize;

use super::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub timeout_seconds: u64,
    pub prompts_dir: String,
    pub retry: LlmRetryConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmRetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl LlmConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&content).map_err(ConfigError::Parse)
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
            .join("tests/fixtures/config/llm.toml")
    }

    #[test]
    fn parse_llm_config() {
        let config = LlmConfig::load(&fixture_path()).unwrap();
        assert_eq!(config.model, "claude-opus-4-20250514");
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.temperature, 0.3);
        assert_eq!(config.timeout_seconds, 120);
        assert_eq!(config.prompts_dir, "prompts");
        assert_eq!(config.retry.max_retries, 3);
        assert_eq!(config.retry.base_delay_ms, 1000);
        assert_eq!(config.retry.max_delay_ms, 30000);
    }

    #[test]
    fn missing_file_returns_io_error() {
        let result = LlmConfig::load(Path::new("/nonexistent/llm.toml"));
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }
}
