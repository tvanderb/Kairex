pub mod api_types;
pub mod client;
pub mod schemas;
pub mod types;

pub use client::{AnthropicClient, LlmResponse};
pub use schemas::{AlertReport, EveningReport, MiddayReport, MorningReport, WeeklyReport};
pub use types::*;

/// Errors from LLM API calls and response handling.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization/deserialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("API returned error: {status} — {message}")]
    Api { status: u16, message: String },

    #[error("API rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("Prompt or schema file not found: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Max retries ({attempts}) exhausted: {message}")]
    RetriesExhausted { attempts: u32, message: String },
}

/// Report types matching scheduler event strings and schema files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportType {
    Morning,
    Midday,
    Evening,
    Alert,
    Weekly,
}

impl ReportType {
    /// Parse from scheduler's report_type string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "morning" => Some(Self::Morning),
            "midday" => Some(Self::Midday),
            "evening" => Some(Self::Evening),
            "alert" => Some(Self::Alert),
            "weekly" => Some(Self::Weekly),
            _ => None,
        }
    }

    /// Tool name matching the schema file (e.g. "morning_report").
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Morning => "morning_report",
            Self::Midday => "midday_report",
            Self::Evening => "evening_report",
            Self::Alert => "alert_report",
            Self::Weekly => "weekly_report",
        }
    }

    /// Schema file path relative to project root.
    pub fn schema_path(&self) -> &'static str {
        match self {
            Self::Morning => "schemas/morning.json",
            Self::Midday => "schemas/midday.json",
            Self::Evening => "schemas/evening.json",
            Self::Alert => "schemas/alert.json",
            Self::Weekly => "schemas/weekly.json",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/llm/{name}",
            env!("CARGO_MANIFEST_DIR").trim_end_matches("/kairex")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
    }

    #[test]
    fn deserialize_morning_report() {
        let json = load_fixture("morning_report.json");
        let report: MorningReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.regime_status, "range_bound");
        assert_eq!(report.regime_duration_days, 4);
        assert_eq!(report.assets.len(), 5);
        assert_eq!(report.setups.len(), 2);
        assert_eq!(report.setups[0].asset, "ETHUSDT");
        assert_eq!(report.setups[0].direction, "short");
        assert_eq!(report.setups[0].trigger_level, 1880.0);
        assert!(report.setups[0].trigger_field.is_none());
        assert_eq!(report.setups[0].confidence, Some(0.72));
        assert_eq!(report.significance.magnitude, 0.4);
    }

    #[test]
    fn deserialize_midday_report() {
        let json = load_fixture("midday_report.json");
        let report: MiddayReport = serde_json::from_str(&json).unwrap();
        assert!(!report.morning_reference_narrative.is_empty());
        assert_eq!(report.assets.len(), 3);
        assert_eq!(report.setups.len(), 2);
        assert_eq!(report.setups[0].confidence, Some(0.78));
    }

    #[test]
    fn deserialize_evening_report() {
        let json = load_fixture("evening_report.json");
        let report: EveningReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.regime_status, "range_bound");
        assert_eq!(report.scorecard.len(), 2);

        let hit = &report.scorecard[0];
        assert_eq!(hit.outcome, "triggered");
        assert_eq!(hit.assessment, "hit");
        assert!(hit.miss_reason.is_none());
        assert_eq!(hit.outcome_price, Some(1838.0));

        let pending = &report.scorecard[1];
        assert_eq!(pending.outcome, "active");
        assert_eq!(pending.assessment, "pending");
        assert!(pending.outcome_price.is_none());

        assert_eq!(report.setups.len(), 2);
        assert!(!report.overnight_narrative.is_empty());
    }

    #[test]
    fn deserialize_alert_report() {
        let json = load_fixture("alert_report.json");
        let report: AlertReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.asset, "ETHUSDT");
        assert!(!report.trigger_summary.is_empty());
        assert!(!report.context_narrative.is_empty());
        assert!(!report.watch_narrative.is_empty());
        assert_eq!(report.setups.len(), 1);
        assert_eq!(report.significance.magnitude, 0.75);
    }

    #[test]
    fn deserialize_weekly_report() {
        let json = load_fixture("weekly_report.json");
        let report: WeeklyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.regime_status, "range_bound");
        assert_eq!(report.regime_duration_days, 11);

        // Scorecard summary
        assert_eq!(report.scorecard_summary.total_setups, 14);
        assert_eq!(report.scorecard_summary.triggered, 6);
        assert_eq!(report.scorecard_summary.hit_rate, 0.67);
        assert_eq!(report.scorecard_summary.by_confidence.len(), 3);
        assert_eq!(report.scorecard_summary.by_confidence[0].level, "high");

        // Notebook
        assert!(!report.notebook.beliefs.is_empty());
        assert!(report.notebook.beliefs.len() <= 8);
        assert!(!report.notebook.biases.is_empty());
        assert!(report.notebook.biases.len() <= 5);
        assert!(!report.notebook.hypotheses.is_empty());
        assert!(report.notebook.hypotheses.len() <= 6);

        // Setups with indicator trigger
        let indicator_setup = report.setups.iter().find(|s| s.trigger_field.is_some());
        assert!(indicator_setup.is_some());
        let setup = indicator_setup.unwrap();
        assert_eq!(setup.trigger_condition, "indicator_below");
        assert_eq!(setup.trigger_field.as_deref(), Some("rsi_14_1h"));
        assert_eq!(setup.trigger_level, 30.0);

        assert!(!report.what_would_change_my_mind.is_empty());
        assert!(!report.regime_assessment.is_empty());
        assert_eq!(report.assets.len(), 5);
    }

    #[test]
    fn forward_compatibility_missing_optional_fields() {
        // A minimal setup with only required fields — optional fields should default
        let json = r#"{
            "asset": "BTCUSDT",
            "direction": "long",
            "trigger_condition": "price_above",
            "trigger_level": 70000.0,
            "narrative": "Test setup"
        }"#;
        let setup: Setup = serde_json::from_str(json).unwrap();
        assert!(setup.trigger_field.is_none());
        assert!(setup.target_level.is_none());
        assert!(setup.invalidation_level.is_none());
        assert!(setup.confidence.is_none());
        assert!(setup.timeframe.is_none());
    }

    #[test]
    fn forward_compatibility_extra_fields_ignored() {
        // A setup with an unknown field — should parse without error
        let json = r#"{
            "asset": "BTCUSDT",
            "direction": "long",
            "trigger_condition": "price_above",
            "trigger_level": 70000.0,
            "narrative": "Test setup",
            "unknown_future_field": "should be ignored"
        }"#;
        // serde default behavior ignores unknown fields
        let setup: Setup = serde_json::from_str(json).unwrap();
        assert_eq!(setup.asset, "BTCUSDT");
    }

    #[test]
    fn scorecard_entry_with_miss_reason() {
        let json = r#"{
            "asset": "BTCUSDT",
            "direction": "long",
            "trigger_level": 70000.0,
            "outcome": "invalidated",
            "outcome_price": 65500.0,
            "assessment": "miss",
            "miss_reason": "wrong_direction",
            "narrative": "Called long but it broke down."
        }"#;
        let entry: ScorecardEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.assessment, "miss");
        assert_eq!(entry.miss_reason.as_deref(), Some("wrong_direction"));
    }

    #[test]
    fn scorecard_summary_empty_by_confidence() {
        // by_confidence is optional via #[serde(default)]
        let json = r#"{
            "total_setups": 5,
            "triggered": 3,
            "invalidated": 1,
            "expired": 1,
            "hit_rate": 0.67,
            "narrative": "Decent week."
        }"#;
        let summary: ScorecardSummary = serde_json::from_str(json).unwrap();
        assert!(summary.by_confidence.is_empty());
    }

    #[test]
    fn roundtrip_serialization() {
        // Ensure we can serialize and deserialize without loss
        let json = load_fixture("morning_report.json");
        let report: MorningReport = serde_json::from_str(&json).unwrap();
        let serialized = serde_json::to_string(&report).unwrap();
        let deserialized: MorningReport = serde_json::from_str(&serialized).unwrap();
        assert_eq!(report, deserialized);
    }
}
