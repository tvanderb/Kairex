use std::path::Path;

use serde::Deserialize;

use super::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct EvaluationConfig {
    pub cycle_interval_seconds: u64,
    pub cooldown_minutes: CooldownConfig,
    #[serde(default)]
    pub startup_expiry_minutes: Option<u64>,
    #[serde(default)]
    pub startup_delay_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CooldownConfig {
    pub setup_trigger: u64,
    pub setup_invalidation: u64,
}

impl EvaluationConfig {
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
            .join("tests/fixtures/config/evaluation.toml")
    }

    #[test]
    fn parse_evaluation_fixture() {
        let config = EvaluationConfig::load(&fixture_path()).unwrap();
        assert_eq!(config.cycle_interval_seconds, 60);
        assert_eq!(config.cooldown_minutes.setup_trigger, 30);
        assert_eq!(config.cooldown_minutes.setup_invalidation, 30);
    }

    #[test]
    fn missing_file_returns_io_error() {
        let result = EvaluationConfig::load(Path::new("/nonexistent/evaluation.toml"));
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not valid { toml").unwrap();
        let result = EvaluationConfig::load(tmp.path());
        assert!(matches!(result, Err(ConfigError::Parse(_))));
    }
}
