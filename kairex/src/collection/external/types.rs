use serde::Deserialize;

// -- Alternative.me Fear & Greed Index --

#[derive(Debug, Clone, Deserialize)]
pub struct FearGreedResponse {
    pub data: Vec<FearGreedEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FearGreedEntry {
    pub value: String,
    pub value_classification: String,
    pub timestamp: String,
}

// -- CoinGecko Global --

#[derive(Debug, Clone, Deserialize)]
pub struct CoinGeckoGlobalResponse {
    pub data: CoinGeckoGlobalData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinGeckoGlobalData {
    pub market_cap_percentage: std::collections::HashMap<String, f64>,
    pub total_market_cap: std::collections::HashMap<String, f64>,
}
