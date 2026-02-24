use std::path::Path;

use serde_json::json;
use tracing::debug;

use super::error::{AnalysisError, Result};
use super::subprocess::run_python_script;
use crate::config::AnalysisConfig;
use crate::storage::Database;

/// Number of candles to feed per timeframe (matches manifest.toml).
const CANDLES_PER_TIMEFRAME: usize = 260;

/// Timeframes to compute indicators for.
const TIMEFRAMES: &[&str] = &["5m", "1h", "1d"];

/// Compute indicators for all assets across all timeframes.
///
/// 1. Queries candle data from storage (260 per symbol/timeframe)
/// 2. Calls `compute_indicators.py` as a subprocess
/// 3. Trims output to `context_periods` per the analysis config
///
/// Returns the raw JSON output from the Python script, trimmed.
pub async fn compute_indicators(
    db: &Database,
    assets: &[String],
    config: &AnalysisConfig,
    project_root: &Path,
) -> Result<serde_json::Value> {
    let input = assemble_input(db, assets).await?;

    debug!(
        assets = assets.len(),
        timeframes = TIMEFRAMES.len(),
        "calling compute_indicators.py"
    );

    let output = run_python_script(
        project_root,
        &config.indicators.python_venv,
        "compute_indicators.py",
        &input,
        config.indicators.compute_timeout_seconds,
    )
    .await?;

    let trimmed = trim_periods(output, config.indicators.context_periods)?;
    Ok(trimmed)
}

/// Assemble the input JSON for compute_indicators.py from storage.
async fn assemble_input(db: &Database, assets: &[String]) -> Result<serde_json::Value> {
    let db = db.clone();
    let assets = assets.to_vec();

    let result = tokio::task::spawn_blocking(move || {
        let now = now_ms();
        let mut candles_map = serde_json::Map::new();

        for asset in &assets {
            let mut tf_map = serde_json::Map::new();

            for &tf in TIMEFRAMES {
                let interval_ms = timeframe_to_ms(tf);
                let start = now - (CANDLES_PER_TIMEFRAME as i64) * interval_ms;

                let candles = db
                    .query_candles(asset, tf, start, now)
                    .map_err(AnalysisError::Storage)?;

                let candle_array: Vec<serde_json::Value> = candles
                    .iter()
                    .map(|c| {
                        json!({
                            "ts": c.open_time,
                            "o": c.open,
                            "h": c.high,
                            "l": c.low,
                            "c": c.close,
                            "v": c.volume,
                        })
                    })
                    .collect();

                tf_map.insert(tf.to_string(), serde_json::Value::Array(candle_array));
            }

            candles_map.insert(asset.clone(), serde_json::Value::Object(tf_map));
        }

        let input = json!({
            "assets": assets,
            "candles": candles_map,
        });

        Ok::<_, AnalysisError>(input)
    })
    .await??;

    Ok(result)
}

/// Trim each symbol/timeframe's periods array to keep only the last `n` entries.
fn trim_periods(mut output: serde_json::Value, n: usize) -> Result<serde_json::Value> {
    let obj = output
        .as_object_mut()
        .ok_or_else(|| AnalysisError::Config("expected top-level JSON object".into()))?;

    for (_asset, tf_data) in obj.iter_mut() {
        if let Some(tf_obj) = tf_data.as_object_mut() {
            for (_tf, tf_value) in tf_obj.iter_mut() {
                if let Some(inner) = tf_value.as_object_mut() {
                    if let Some(periods) = inner.get_mut("periods") {
                        if let Some(arr) = periods.as_array_mut() {
                            if arr.len() > n {
                                let start = arr.len() - n;
                                *arr = arr.split_off(start);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(output)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn timeframe_to_ms(tf: &str) -> i64 {
    match tf {
        "5m" => 5 * 60 * 1000,
        "1h" => 60 * 60 * 1000,
        "1d" => 24 * 60 * 60 * 1000,
        _ => 60 * 60 * 1000, // default to 1h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeframe_conversions() {
        assert_eq!(timeframe_to_ms("5m"), 300_000);
        assert_eq!(timeframe_to_ms("1h"), 3_600_000);
        assert_eq!(timeframe_to_ms("1d"), 86_400_000);
    }

    #[test]
    fn trim_periods_truncates_to_n() {
        let input = json!({
            "BTCUSDT": {
                "1h": {
                    "periods": [
                        {"ts": 1, "sma_20": 100.0},
                        {"ts": 2, "sma_20": 101.0},
                        {"ts": 3, "sma_20": 102.0},
                        {"ts": 4, "sma_20": 103.0},
                        {"ts": 5, "sma_20": 104.0},
                    ]
                }
            }
        });

        let result = trim_periods(input, 3).unwrap();
        let periods = result["BTCUSDT"]["1h"]["periods"].as_array().unwrap();
        assert_eq!(periods.len(), 3);
        assert_eq!(periods[0]["ts"], 3);
        assert_eq!(periods[2]["ts"], 5);
    }

    #[test]
    fn trim_periods_noop_when_within_limit() {
        let input = json!({
            "BTCUSDT": {
                "1h": {
                    "periods": [
                        {"ts": 1},
                        {"ts": 2},
                    ]
                }
            }
        });

        let result = trim_periods(input, 5).unwrap();
        let periods = result["BTCUSDT"]["1h"]["periods"].as_array().unwrap();
        assert_eq!(periods.len(), 2);
    }
}
