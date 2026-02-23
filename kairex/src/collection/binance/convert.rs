use crate::storage::{Candle, FundingRate, OpenInterest};

use super::types::{BinanceFundingRate, BinanceOpenInterest, KlineData};

/// Parse a Binance kline JSON array (from /api/v3/klines) into a Candle.
///
/// Array format: [open_time, open, high, low, close, volume, close_time,
///                quote_vol, trades, taker_buy_base, taker_buy_quote, ignore]
/// Types are mixed: timestamps are i64, prices/volumes are strings.
pub fn kline_array_to_candle(
    arr: &[serde_json::Value],
    symbol: &str,
    timeframe: &str,
) -> Option<Candle> {
    if arr.len() < 12 {
        return None;
    }

    Some(Candle {
        symbol: symbol.to_string(),
        timeframe: timeframe.to_string(),
        open_time: arr[0].as_i64()?,
        open: parse_decimal(&arr[1])?,
        high: parse_decimal(&arr[2])?,
        low: parse_decimal(&arr[3])?,
        close: parse_decimal(&arr[4])?,
        volume: parse_decimal(&arr[5])?,
        source: "rest".to_string(),
    })
}

/// Convert a WebSocket KlineData to a Candle.
pub fn ws_kline_to_candle(kline: &KlineData) -> Option<Candle> {
    Some(Candle {
        symbol: kline.symbol.clone(),
        timeframe: kline.interval.clone(),
        open_time: kline.open_time,
        open: kline.open.parse().ok()?,
        high: kline.high.parse().ok()?,
        low: kline.low.parse().ok()?,
        close: kline.close.parse().ok()?,
        volume: kline.volume.parse().ok()?,
        source: "ws".to_string(),
    })
}

/// Convert a Binance funding rate response to storage model.
pub fn binance_funding_to_model(rate: &BinanceFundingRate) -> Option<FundingRate> {
    Some(FundingRate {
        symbol: rate.symbol.clone(),
        timestamp: rate.funding_time,
        rate: rate.funding_rate.parse().ok()?,
    })
}

/// Convert a Binance open interest response to storage model.
pub fn binance_oi_to_model(oi: &BinanceOpenInterest) -> Option<OpenInterest> {
    Some(OpenInterest {
        symbol: oi.symbol.clone(),
        timestamp: oi.time,
        value: oi.open_interest.parse().ok()?,
    })
}

/// Parse a JSON value that might be a string or number into f64.
fn parse_decimal(val: &serde_json::Value) -> Option<f64> {
    match val {
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kline_array_parses_correctly() {
        let arr = json!([
            1708992000000_i64,
            "51234.56",
            "51500.00",
            "51000.00",
            "51300.00",
            "1234.567",
            1708992299999_i64,
            "63234567.89",
            5432,
            "617.283",
            "31617283.94",
            "0"
        ]);
        let arr = arr.as_array().unwrap();

        let candle = kline_array_to_candle(arr, "BTCUSDT", "5m").unwrap();
        assert_eq!(candle.symbol, "BTCUSDT");
        assert_eq!(candle.timeframe, "5m");
        assert_eq!(candle.open_time, 1708992000000);
        assert!((candle.open - 51234.56).abs() < 0.01);
        assert!((candle.high - 51500.00).abs() < 0.01);
        assert!((candle.low - 51000.00).abs() < 0.01);
        assert!((candle.close - 51300.00).abs() < 0.01);
        assert!((candle.volume - 1234.567).abs() < 0.001);
        assert_eq!(candle.source, "rest");
    }

    #[test]
    fn kline_array_too_short_returns_none() {
        let arr = json!([1, 2, 3]);
        let arr = arr.as_array().unwrap();
        assert!(kline_array_to_candle(arr, "BTCUSDT", "5m").is_none());
    }

    #[test]
    fn funding_rate_conversion() {
        let raw = BinanceFundingRate {
            symbol: "BTCUSDT".into(),
            funding_time: 1708992000000,
            funding_rate: "0.0001".into(),
            mark_price: "51234.56".into(),
        };
        let model = binance_funding_to_model(&raw).unwrap();
        assert_eq!(model.symbol, "BTCUSDT");
        assert_eq!(model.timestamp, 1708992000000);
        assert!((model.rate - 0.0001).abs() < 1e-10);
    }

    #[test]
    fn open_interest_conversion() {
        let raw = BinanceOpenInterest {
            symbol: "BTCUSDT".into(),
            open_interest: "12345.678".into(),
            time: 1708992000000,
        };
        let model = binance_oi_to_model(&raw).unwrap();
        assert_eq!(model.symbol, "BTCUSDT");
        assert_eq!(model.timestamp, 1708992000000);
        assert!((model.value - 12345.678).abs() < 0.001);
    }

    #[test]
    fn ws_kline_conversion() {
        let kline = KlineData {
            open_time: 1708992000000,
            close_time: 1708992299999,
            symbol: "BTCUSDT".into(),
            interval: "5m".into(),
            open: "51234.56".into(),
            high: "51500.00".into(),
            low: "51000.00".into(),
            close: "51300.00".into(),
            volume: "1234.567".into(),
            is_final: true,
        };
        let candle = ws_kline_to_candle(&kline).unwrap();
        assert_eq!(candle.symbol, "BTCUSDT");
        assert_eq!(candle.timeframe, "5m");
        assert_eq!(candle.source, "ws");
        assert!((candle.open - 51234.56).abs() < 0.01);
    }

    #[test]
    fn parse_decimal_handles_string_and_number() {
        assert!((parse_decimal(&json!("123.456")).unwrap() - 123.456).abs() < 0.001);
        assert!((parse_decimal(&json!(123.456)).unwrap() - 123.456).abs() < 0.001);
        assert!(parse_decimal(&json!(null)).is_none());
    }
}
