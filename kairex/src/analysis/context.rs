use std::path::Path;

use serde_json::json;
use tracing::{debug, instrument};

use super::error::{AnalysisError, Result};
use super::subprocess::run_python_script;
use crate::config::AnalysisConfig;
use crate::storage::Database;

/// Data requirements from manifest.toml for build_context.
const CANDLES_5M_PERIODS: usize = 288; // 24h
const CANDLES_1H_PERIODS: usize = 168; // 7d
const CANDLES_1D_PERIODS: usize = 90; // 90d
const FUNDING_RATE_PERIODS: usize = 90;
const OPEN_INTEREST_PERIODS: usize = 720; // 30d hourly
const INDEX_PERIODS: usize = 90;

const INDEX_TYPES: &[&str] = &[
    "fear_greed",
    "btc_dominance",
    "eth_dominance",
    "total_market_cap",
];

/// Build the numerical context for an LLM report.
///
/// 1. Queries candles, funding rates, open interest, and indices from storage
/// 2. Calls `build_context.py` as a subprocess
/// 3. Returns the structured context JSON
#[instrument(name = "analysis.build_context", skip_all, fields(assets = assets.len()))]
pub async fn build_context(
    db: &Database,
    assets: &[String],
    config: &AnalysisConfig,
    project_root: &Path,
) -> Result<serde_json::Value> {
    let input = assemble_input(db, assets).await?;

    debug!(assets = assets.len(), "calling build_context.py");

    run_python_script(
        project_root,
        &config.indicators.python_venv,
        "build_context.py",
        &input,
        config.indicators.context_timeout_seconds,
    )
    .await
}

/// Assemble the input JSON for build_context.py from storage.
async fn assemble_input(db: &Database, assets: &[String]) -> Result<serde_json::Value> {
    let db = db.clone();
    let assets = assets.to_vec();

    let result = tokio::task::spawn_blocking(move || {
        let now = now_ms();

        // Candles: per-asset, per-timeframe
        let mut candles_map = serde_json::Map::new();
        for asset in &assets {
            let mut tf_map = serde_json::Map::new();

            for (tf, periods, interval_ms) in [
                ("5m", CANDLES_5M_PERIODS, 300_000i64),
                ("1h", CANDLES_1H_PERIODS, 3_600_000),
                ("1d", CANDLES_1D_PERIODS, 86_400_000),
            ] {
                let start = now - (periods as i64) * interval_ms;
                let candles = db
                    .query_candles(asset, tf, start, now)
                    .map_err(AnalysisError::Storage)?;

                let arr: Vec<serde_json::Value> = candles
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

                tf_map.insert(tf.to_string(), serde_json::Value::Array(arr));
            }

            candles_map.insert(asset.clone(), serde_json::Value::Object(tf_map));
        }

        // Funding rates: per-asset
        let mut funding_map = serde_json::Map::new();
        let funding_start = now - (FUNDING_RATE_PERIODS as i64) * 86_400_000; // 90 days
        for asset in &assets {
            let rates = db
                .query_funding_rates(asset, funding_start, now)
                .map_err(AnalysisError::Storage)?;

            let arr: Vec<serde_json::Value> = rates
                .iter()
                .map(|r| json!({"ts": r.timestamp, "rate": r.rate}))
                .collect();

            funding_map.insert(asset.clone(), serde_json::Value::Array(arr));
        }

        // Open interest: per-asset
        let mut oi_map = serde_json::Map::new();
        let oi_start = now - (OPEN_INTEREST_PERIODS as i64) * 3_600_000; // 720 hours
        for asset in &assets {
            let oi = db
                .query_open_interest(asset, oi_start, now)
                .map_err(AnalysisError::Storage)?;

            let arr: Vec<serde_json::Value> = oi
                .iter()
                .map(|o| json!({"ts": o.timestamp, "value": o.value}))
                .collect();

            oi_map.insert(asset.clone(), serde_json::Value::Array(arr));
        }

        // Indices: per-type
        let mut indices_map = serde_json::Map::new();
        let index_start = now - (INDEX_PERIODS as i64) * 86_400_000; // 90 days
        for &idx_type in INDEX_TYPES {
            let values = db
                .query_index_values(idx_type, index_start, now)
                .map_err(AnalysisError::Storage)?;

            let arr: Vec<serde_json::Value> = values
                .iter()
                .map(|v| json!({"ts": v.timestamp, "value": v.value}))
                .collect();

            indices_map.insert(idx_type.to_string(), serde_json::Value::Array(arr));
        }

        let input = json!({
            "assets": assets,
            "candles": candles_map,
            "funding_rates": funding_map,
            "open_interest": oi_map,
            "indices": indices_map,
        });

        Ok::<_, AnalysisError>(input)
    })
    .await??;

    Ok(result)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
