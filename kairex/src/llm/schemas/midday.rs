use serde::{Deserialize, Serialize};

use crate::llm::types::{AssetNarrative, Setup, Significance};

/// Structured output for the midday update report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MiddayReport {
    pub morning_reference_narrative: String,
    pub assets: Vec<AssetNarrative>,
    pub setups: Vec<Setup>,
    pub market_narrative: String,
    pub significance: Significance,
}
