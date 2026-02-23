use std::fmt;

#[derive(Debug, Clone)]
pub struct CollectionEvent {
    pub source: EventSource,
    pub symbol: Option<String>,
    pub data_type: DataType,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSource {
    BinanceRest,
    BinanceWebSocket,
    AlternativeMe,
    CoinGecko,
}

impl fmt::Display for EventSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinanceRest => write!(f, "binance_rest"),
            Self::BinanceWebSocket => write!(f, "binance_ws"),
            Self::AlternativeMe => write!(f, "alternative_me"),
            Self::CoinGecko => write!(f, "coingecko"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    Candle { timeframe: String },
    FundingRate,
    OpenInterest,
    Index { index_type: String },
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Candle { timeframe } => write!(f, "candle_{timeframe}"),
            Self::FundingRate => write!(f, "funding_rate"),
            Self::OpenInterest => write!(f, "open_interest"),
            Self::Index { index_type } => write!(f, "index_{index_type}"),
        }
    }
}
