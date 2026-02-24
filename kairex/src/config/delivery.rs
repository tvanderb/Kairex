use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use super::ConfigError;

// -- DeliveryConfig (from delivery.toml) --

#[derive(Debug, Clone, Deserialize)]
pub struct DeliveryConfig {
    pub setup_format: SetupFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupFormat {
    DetailLine,
    InlineCompact,
    Card,
}

impl DeliveryConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&content).map_err(ConfigError::Parse)
    }
}

// -- FreeChannelConfig (from free_channel.toml) --

#[derive(Debug, Clone, Deserialize)]
pub struct FreeChannelConfig {
    #[serde(flatten)]
    pub routes: HashMap<String, RouteConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteConfig {
    pub route: RouteMode,
    #[serde(default)]
    pub format: FormatMode,
    #[serde(default)]
    pub rules: Vec<RouteRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    Always,
    Threshold,
    Never,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatMode {
    PassThrough,
    #[default]
    Condensed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteRule {
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    pub op: String,
    pub value: f64,
}

impl FreeChannelConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&content).map_err(ConfigError::Parse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tests/fixtures/config")
    }

    #[test]
    fn parse_delivery_config() {
        let config = DeliveryConfig::load(&fixture_dir().join("delivery.toml")).unwrap();
        assert_eq!(config.setup_format, SetupFormat::DetailLine);
    }

    #[test]
    fn parse_delivery_config_missing_file() {
        let result = DeliveryConfig::load(Path::new("/nonexistent/delivery.toml"));
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }

    #[test]
    fn parse_all_setup_format_variants() {
        let config: DeliveryConfig = toml::from_str("setup_format = \"detail_line\"").unwrap();
        assert_eq!(config.setup_format, SetupFormat::DetailLine);

        let config: DeliveryConfig = toml::from_str("setup_format = \"inline_compact\"").unwrap();
        assert_eq!(config.setup_format, SetupFormat::InlineCompact);

        let config: DeliveryConfig = toml::from_str("setup_format = \"card\"").unwrap();
        assert_eq!(config.setup_format, SetupFormat::Card);
    }

    #[test]
    fn parse_free_channel_config() {
        let config = FreeChannelConfig::load(&fixture_dir().join("free_channel.toml")).unwrap();

        assert_eq!(config.routes.len(), 4);

        // evening_recap: always, condensed
        let evening = &config.routes["evening_recap"];
        assert_eq!(evening.route, RouteMode::Always);
        assert_eq!(evening.format, FormatMode::Condensed);
        assert!(evening.rules.is_empty());

        // weekly_scorecard: always, pass_through
        let weekly = &config.routes["weekly_scorecard"];
        assert_eq!(weekly.route, RouteMode::Always);
        assert_eq!(weekly.format, FormatMode::PassThrough);

        // morning_report: threshold with 2 rules
        let morning = &config.routes["morning_report"];
        assert_eq!(morning.route, RouteMode::Threshold);
        assert_eq!(morning.rules.len(), 2);
        assert_eq!(morning.rules[0].field.as_deref(), Some("magnitude"));
        assert_eq!(morning.rules[0].op, ">");
        assert_eq!(morning.rules[0].value, 0.7);
        assert_eq!(
            morning.rules[1].fields.as_deref(),
            Some(&["surprise".to_string(), "regime_relevance".to_string()][..])
        );
        assert_eq!(morning.rules[1].op, "all_above");
        assert_eq!(morning.rules[1].value, 0.5);

        // alerts: threshold with 2 rules
        let alerts = &config.routes["alerts"];
        assert_eq!(alerts.route, RouteMode::Threshold);
        assert_eq!(alerts.rules.len(), 2);
    }

    #[test]
    fn parse_free_channel_config_missing_file() {
        let result = FreeChannelConfig::load(Path::new("/nonexistent/free_channel.toml"));
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }

    #[test]
    fn format_mode_defaults_to_condensed() {
        let toml_str = r#"
            [test_section]
            route = "always"
        "#;
        let config: FreeChannelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.routes["test_section"].format, FormatMode::Condensed);
    }
}
