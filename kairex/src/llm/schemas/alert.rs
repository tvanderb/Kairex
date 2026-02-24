use serde::{Deserialize, Serialize};

use crate::llm::types::{Setup, Significance};

/// Structured output for a real-time alert.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertReport {
    pub asset: String,
    pub trigger_summary: String,
    pub context_narrative: String,
    pub watch_narrative: String,
    pub setups: Vec<Setup>,
    pub significance: Significance,
}
