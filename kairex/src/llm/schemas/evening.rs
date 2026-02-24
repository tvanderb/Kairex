use serde::{Deserialize, Serialize};

use crate::llm::types::{ScorecardEntry, Setup, Significance};

/// Structured output for the evening recap report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EveningReport {
    pub regime_status: String,
    pub regime_duration_days: i32,
    pub regime_narrative: String,
    pub scorecard: Vec<ScorecardEntry>,
    pub setups: Vec<Setup>,
    pub overnight_narrative: String,
    pub market_narrative: String,
    pub significance: Significance,
}
