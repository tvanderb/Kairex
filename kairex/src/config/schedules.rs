use std::path::Path;

use serde::Deserialize;

use super::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulesConfig {
    pub morning: ReportSchedule,
    pub midday: ReportSchedule,
    pub evening: ReportSchedule,
    pub weekly: WeeklySchedule,
    pub overnight: OvernightConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportSchedule {
    pub delivery_time: String,
    pub generation_buffer_minutes: u32,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeeklySchedule {
    pub delivery_time: String,
    pub day: String,
    pub generation_buffer_minutes: u32,
    pub description: String,
    #[serde(default)]
    pub skip_daily_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OvernightConfig {
    pub start: String,
    pub end: String,
}

impl SchedulesConfig {
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
            .join("tests/fixtures/config/schedules.toml")
    }

    #[test]
    fn parse_schedules_fixture() {
        let config = SchedulesConfig::load(&fixture_path()).unwrap();
        assert_eq!(config.morning.delivery_time, "08:30");
        assert_eq!(config.morning.generation_buffer_minutes, 10);
        assert_eq!(config.midday.delivery_time, "12:00");
        assert_eq!(config.evening.delivery_time, "20:00");
        assert_eq!(config.weekly.delivery_time, "17:00");
        assert_eq!(config.weekly.day, "sunday");
        assert_eq!(config.weekly.generation_buffer_minutes, 15);
        assert_eq!(config.weekly.skip_daily_on, vec!["sunday"]);
        assert_eq!(config.overnight.start, "22:00");
        assert_eq!(config.overnight.end, "08:00");
    }

    #[test]
    fn missing_file_returns_io_error() {
        let result = SchedulesConfig::load(Path::new("/nonexistent/schedules.toml"));
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }
}
