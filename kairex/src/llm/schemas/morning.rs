use serde::{Deserialize, Serialize};

use crate::llm::types::{AssetNarrative, Setup, Significance};

/// Structured output for the pre-market morning report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MorningReport {
    pub regime_status: String,
    pub regime_duration_days: i32,
    pub regime_narrative: String,
    pub assets: Vec<AssetNarrative>,
    pub setups: Vec<Setup>,
    pub market_narrative: String,
    pub significance: Significance,
}
