use std::path::Path;

use serde::Deserialize;

use super::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisConfig {
    pub indicators: IndicatorsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndicatorsConfig {
    pub context_periods: usize,
    pub compute_timeout_seconds: u64,
    pub context_timeout_seconds: u64,
    pub python_venv: String,
}

impl AnalysisConfig {
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
            .join("tests/fixtures/config/analysis.toml")
    }

    #[test]
    fn parse_analysis_fixture() {
        let config = AnalysisConfig::load(&fixture_path()).unwrap();
        assert_eq!(config.indicators.context_periods, 17);
        assert_eq!(config.indicators.compute_timeout_seconds, 30);
        assert_eq!(config.indicators.context_timeout_seconds, 60);
        assert_eq!(config.indicators.python_venv, ".venv");
    }

    #[test]
    fn missing_file_returns_io_error() {
        let result = AnalysisConfig::load(Path::new("/nonexistent/analysis.toml"));
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }
}
