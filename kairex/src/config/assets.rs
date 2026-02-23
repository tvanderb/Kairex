use std::path::Path;

use serde::Deserialize;

use super::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct AssetsConfig {
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub symbol: String,
    pub display: String,
}

impl AssetsConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn symbols(&self) -> Vec<&str> {
        self.assets.iter().map(|a| a.symbol.as_str()).collect()
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
            .join("tests/fixtures/config/assets.toml")
    }

    #[test]
    fn parse_assets_fixture() {
        let config = AssetsConfig::load(&fixture_path()).unwrap();
        assert_eq!(config.assets.len(), 9);
        assert_eq!(config.assets[0].symbol, "BTCUSDT");
        assert_eq!(config.assets[0].display, "BTC");
        assert_eq!(config.assets[1].symbol, "ETHUSDT");
        assert_eq!(config.assets[1].display, "ETH");
    }

    #[test]
    fn symbols_returns_all() {
        let config = AssetsConfig::load(&fixture_path()).unwrap();
        let symbols = config.symbols();
        assert_eq!(symbols.len(), 9);
        assert!(symbols.contains(&"BTCUSDT"));
        assert!(symbols.contains(&"DOTUSDT"));
    }

    #[test]
    fn missing_file_returns_io_error() {
        let result = AssetsConfig::load(Path::new("/nonexistent/path.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, super::super::ConfigError::Io(_)));
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not valid { toml").unwrap();
        let result = AssetsConfig::load(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, super::super::ConfigError::Parse(_)));
    }
}
