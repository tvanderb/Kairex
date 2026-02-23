use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candle {
    pub symbol: String,
    pub timeframe: String,
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FundingRate {
    pub symbol: String,
    pub timestamp: i64,
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenInterest {
    pub symbol: String,
    pub timestamp: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexValue {
    pub index_type: String,
    pub timestamp: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemOutput {
    pub id: Option<i64>,
    pub report_type: String,
    pub generated_at: i64,
    pub schema_version: String,
    pub output: serde_json::Value,
    pub delivered_at: Option<i64>,
    pub delivery_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveSetup {
    pub id: Option<i64>,
    pub source_output_id: i64,
    pub asset: String,
    pub direction: String,
    pub trigger_condition: String,
    pub trigger_level: f64,
    pub target_level: Option<f64>,
    pub invalidation_level: Option<f64>,
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FiredAlert {
    pub id: Option<i64>,
    pub setup_id: Option<i64>,
    pub alert_type: String,
    pub fired_at: i64,
    pub cooldown_until: i64,
    pub output_id: Option<i64>,
}
