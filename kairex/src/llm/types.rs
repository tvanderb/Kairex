use serde::{Deserialize, Serialize};

/// A tradeable setup with machine-readable trigger conditions.
///
/// Produced by the LLM as part of report output. Trigger conditions
/// are evaluated by the live evaluation loop every 5 minutes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Setup {
    pub asset: String,
    pub direction: String,
    pub trigger_condition: String,
    pub trigger_level: f64,
    #[serde(default)]
    pub trigger_field: Option<String>,
    #[serde(default)]
    pub target_level: Option<f64>,
    #[serde(default)]
    pub invalidation_level: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub timeframe: Option<String>,
    pub narrative: String,
}

/// Analytical significance ratings on every report and alert.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Significance {
    pub magnitude: f64,
    pub surprise: f64,
    pub regime_relevance: f64,
}

/// Structured scoring of a single setup in the evening scorecard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScorecardEntry {
    pub asset: String,
    pub direction: String,
    pub trigger_level: f64,
    pub outcome: String,
    #[serde(default)]
    pub outcome_price: Option<f64>,
    pub assessment: String,
    #[serde(default)]
    pub miss_reason: Option<String>,
    pub narrative: String,
}

/// Per-asset analytical narrative.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetNarrative {
    pub symbol: String,
    pub narrative: String,
}

/// Persistent analyst notebook, updated weekly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notebook {
    pub beliefs: Vec<String>,
    pub biases: Vec<String>,
    pub hypotheses: Vec<String>,
}

/// Aggregated weekly scorecard summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScorecardSummary {
    pub total_setups: u32,
    pub triggered: u32,
    pub invalidated: u32,
    pub expired: u32,
    pub hit_rate: f64,
    #[serde(default)]
    pub by_confidence: Vec<ConfidenceBucket>,
    pub narrative: String,
}

/// Performance breakdown for a confidence level bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfidenceBucket {
    pub level: String,
    pub count: u32,
    pub hit_rate: f64,
}
