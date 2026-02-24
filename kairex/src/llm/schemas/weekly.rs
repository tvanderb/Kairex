use serde::{Deserialize, Serialize};

use crate::llm::types::{AssetNarrative, Notebook, ScorecardSummary, Setup, Significance};

/// Structured output for the Sunday weekly briefing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeeklyReport {
    pub regime_status: String,
    pub regime_duration_days: i32,
    pub regime_narrative: String,
    pub scorecard_summary: ScorecardSummary,
    pub week_narrative: String,
    pub regime_assessment: String,
    pub what_would_change_my_mind: String,
    pub setups: Vec<Setup>,
    pub assets: Vec<AssetNarrative>,
    pub notebook: Notebook,
    pub significance: Significance,
}
