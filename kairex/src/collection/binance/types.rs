use serde::Deserialize;

/// Binance /fapi/v1/fundingRate response item.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFundingRate {
    pub symbol: String,
    pub funding_time: i64,
    pub funding_rate: String,
    #[serde(default)]
    pub mark_price: String,
}

/// Binance /fapi/v1/openInterest response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOpenInterest {
    pub symbol: String,
    pub open_interest: String,
    pub time: i64,
}

// -- WebSocket stream types --

/// Combined stream wrapper: {"stream": "btcusdt@kline_5m", "data": {...}}
#[derive(Debug, Clone, Deserialize)]
pub struct CombinedStreamMessage {
    pub stream: String,
    pub data: KlineStreamEvent,
}

/// Kline stream event payload.
#[derive(Debug, Clone, Deserialize)]
pub struct KlineStreamEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "k")]
    pub kline: KlineData,
}

/// Individual kline data within a stream event.
#[derive(Debug, Clone, Deserialize)]
pub struct KlineData {
    #[serde(rename = "t")]
    pub open_time: i64,
    #[serde(rename = "T")]
    pub close_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub interval: String,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "x")]
    pub is_final: bool,
}
