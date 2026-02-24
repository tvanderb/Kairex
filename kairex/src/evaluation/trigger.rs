use crate::storage::ActiveSetup;

/// Outcome of evaluating a single setup against current market data.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerOutcome {
    /// Trigger condition met. `price` is the close price that triggered it.
    Triggered { price: f64 },
    /// Invalidation level breached. `price` is the close price.
    Invalidated { price: f64 },
    /// Neither triggered nor invalidated.
    Unchanged,
    /// Could not evaluate (e.g. missing indicator data).
    Skipped { reason: String },
}

/// Evaluate a price-based trigger (`price_above` / `price_below`).
///
/// Checks invalidation first (always price-based, direction-aware).
/// If both trigger and invalidation fire simultaneously, invalidation wins.
pub fn evaluate_price_trigger(setup: &ActiveSetup, close_price: f64) -> TriggerOutcome {
    // Check invalidation first (takes priority)
    if let Some(outcome) = check_invalidation(setup, close_price) {
        return outcome;
    }

    match setup.trigger_condition.as_str() {
        "price_above" => {
            if close_price >= setup.trigger_level {
                TriggerOutcome::Triggered { price: close_price }
            } else {
                TriggerOutcome::Unchanged
            }
        }
        "price_below" => {
            if close_price <= setup.trigger_level {
                TriggerOutcome::Triggered { price: close_price }
            } else {
                TriggerOutcome::Unchanged
            }
        }
        _ => TriggerOutcome::Skipped {
            reason: format!(
                "unknown price trigger condition: {}",
                setup.trigger_condition
            ),
        },
    }
}

/// Evaluate an indicator-based trigger (`indicator_above` / `indicator_below`).
///
/// `close_price` is still needed for invalidation (always price-based).
/// `indicator_value` is the computed indicator value, if available.
pub fn evaluate_indicator_trigger(
    setup: &ActiveSetup,
    close_price: f64,
    indicator_value: Option<f64>,
) -> TriggerOutcome {
    // Check invalidation first (always price-based)
    if let Some(outcome) = check_invalidation(setup, close_price) {
        return outcome;
    }

    let value = match indicator_value {
        Some(v) => v,
        None => {
            return TriggerOutcome::Skipped {
                reason: format!(
                    "missing indicator value for {}",
                    setup.trigger_field.as_deref().unwrap_or("unknown")
                ),
            };
        }
    };

    match setup.trigger_condition.as_str() {
        "indicator_above" => {
            if value >= setup.trigger_level {
                TriggerOutcome::Triggered { price: close_price }
            } else {
                TriggerOutcome::Unchanged
            }
        }
        "indicator_below" => {
            if value <= setup.trigger_level {
                TriggerOutcome::Triggered { price: close_price }
            } else {
                TriggerOutcome::Unchanged
            }
        }
        _ => TriggerOutcome::Skipped {
            reason: format!(
                "unknown indicator trigger condition: {}",
                setup.trigger_condition
            ),
        },
    }
}

/// Check invalidation (always price-based, direction-aware).
/// Returns `Some(Invalidated)` if breached, `None` otherwise.
fn check_invalidation(setup: &ActiveSetup, close_price: f64) -> Option<TriggerOutcome> {
    let invalidation_level = setup.invalidation_level?;

    let invalidated = match setup.direction.as_str() {
        "long" => close_price <= invalidation_level,
        "short" => close_price >= invalidation_level,
        _ => false,
    };

    if invalidated {
        Some(TriggerOutcome::Invalidated { price: close_price })
    } else {
        None
    }
}

/// Known timeframe suffixes, ordered longest-first so multi-char suffixes match before shorter ones.
const TIMEFRAME_SUFFIXES: &[&str] = &["_5m", "_1h", "_1d"];

/// Parse a trigger_field like `rsi_14_1h` into `("rsi_14", "1h")`.
///
/// Indicator names can contain underscores (`bollinger_bandwidth`, `stochastic_rsi_k`),
/// so we strip known timeframe suffixes rather than splitting on underscore.
pub fn parse_trigger_field(field: &str) -> Option<(&str, &str)> {
    for suffix in TIMEFRAME_SUFFIXES {
        if let Some(indicator) = field.strip_suffix(suffix) {
            if !indicator.is_empty() {
                let timeframe = &suffix[1..]; // strip leading underscore
                return Some((indicator, timeframe));
            }
        }
    }
    None
}

/// Extract an indicator value from the compute_indicators JSON output.
///
/// Expected structure: `{ "ASSET": { "TIMEFRAME": { "periods": [{ "indicator": value, ... }] } } }`
/// Returns the value from the **last** period (most recent).
pub fn extract_indicator_value(
    json: &serde_json::Value,
    asset: &str,
    timeframe: &str,
    indicator: &str,
) -> Option<f64> {
    let periods = json
        .get(asset)?
        .get(timeframe)?
        .get("periods")?
        .as_array()?;

    let last = periods.last()?;
    let value = last.get(indicator)?;

    // Handle both direct numbers and null
    value.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_setup(
        direction: &str,
        trigger_condition: &str,
        trigger_level: f64,
        invalidation_level: Option<f64>,
    ) -> ActiveSetup {
        ActiveSetup {
            id: Some(1),
            source_output_id: 1,
            asset: "BTCUSDT".into(),
            direction: direction.into(),
            trigger_condition: trigger_condition.into(),
            trigger_level,
            trigger_field: None,
            target_level: Some(75000.0),
            invalidation_level,
            confidence: Some(0.7),
            status: "active".into(),
            created_at: 1000,
            resolved_at: None,
            resolved_price: None,
        }
    }

    fn make_indicator_setup(
        direction: &str,
        trigger_condition: &str,
        trigger_level: f64,
        trigger_field: &str,
        invalidation_level: Option<f64>,
    ) -> ActiveSetup {
        let mut setup = make_setup(
            direction,
            trigger_condition,
            trigger_level,
            invalidation_level,
        );
        setup.trigger_field = Some(trigger_field.into());
        setup
    }

    // --- Price trigger: price_above ---

    #[test]
    fn price_above_triggered_when_close_exceeds_level() {
        let setup = make_setup("long", "price_above", 70000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 71000.0);
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 71000.0 });
    }

    #[test]
    fn price_above_triggered_at_exact_level() {
        let setup = make_setup("long", "price_above", 70000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 70000.0);
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 70000.0 });
    }

    #[test]
    fn price_above_unchanged_when_below_level() {
        let setup = make_setup("long", "price_above", 70000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 69000.0);
        assert_eq!(outcome, TriggerOutcome::Unchanged);
    }

    // --- Price trigger: price_below ---

    #[test]
    fn price_below_triggered_when_close_under_level() {
        let setup = make_setup("short", "price_below", 60000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 59000.0);
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 59000.0 });
    }

    #[test]
    fn price_below_triggered_at_exact_level() {
        let setup = make_setup("short", "price_below", 60000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 60000.0);
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 60000.0 });
    }

    #[test]
    fn price_below_unchanged_when_above_level() {
        let setup = make_setup("short", "price_below", 60000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 61000.0);
        assert_eq!(outcome, TriggerOutcome::Unchanged);
    }

    // --- Invalidation (price triggers) ---

    #[test]
    fn long_invalidated_when_close_at_invalidation_level() {
        let setup = make_setup("long", "price_above", 70000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 65000.0);
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 65000.0 });
    }

    #[test]
    fn long_invalidated_when_close_below_invalidation_level() {
        let setup = make_setup("long", "price_above", 70000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 64000.0);
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 64000.0 });
    }

    #[test]
    fn short_invalidated_when_close_at_invalidation_level() {
        let setup = make_setup("short", "price_below", 60000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 65000.0);
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 65000.0 });
    }

    #[test]
    fn short_invalidated_when_close_above_invalidation_level() {
        let setup = make_setup("short", "price_below", 60000.0, Some(65000.0));
        let outcome = evaluate_price_trigger(&setup, 66000.0);
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 66000.0 });
    }

    // --- Invalidation priority over trigger ---

    #[test]
    fn invalidation_wins_when_both_conditions_met_long() {
        // Long setup where trigger is price_above 70k, invalidation at 70k.
        // Close at 70k: both trigger (>= 70k) and invalidation (<= 70k) fire.
        // Invalidation should win.
        let setup = make_setup("long", "price_above", 70000.0, Some(70000.0));
        let outcome = evaluate_price_trigger(&setup, 70000.0);
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 70000.0 });
    }

    #[test]
    fn invalidation_wins_when_both_conditions_met_short() {
        // Short setup where trigger is price_below 60k, invalidation at 60k.
        // Close at 60k: both fire. Invalidation wins.
        let setup = make_setup("short", "price_below", 60000.0, Some(60000.0));
        let outcome = evaluate_price_trigger(&setup, 60000.0);
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 60000.0 });
    }

    // --- No invalidation level ---

    #[test]
    fn no_invalidation_level_skips_invalidation_check() {
        let setup = make_setup("long", "price_above", 70000.0, None);
        let outcome = evaluate_price_trigger(&setup, 71000.0);
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 71000.0 });
    }

    // --- Indicator triggers ---

    #[test]
    fn indicator_above_triggered() {
        let setup =
            make_indicator_setup("long", "indicator_above", 70.0, "rsi_14_1h", Some(65000.0));
        let outcome = evaluate_indicator_trigger(&setup, 68000.0, Some(75.0));
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 68000.0 });
    }

    #[test]
    fn indicator_above_unchanged() {
        let setup =
            make_indicator_setup("long", "indicator_above", 70.0, "rsi_14_1h", Some(65000.0));
        let outcome = evaluate_indicator_trigger(&setup, 68000.0, Some(65.0));
        assert_eq!(outcome, TriggerOutcome::Unchanged);
    }

    #[test]
    fn indicator_below_triggered() {
        let setup =
            make_indicator_setup("short", "indicator_below", 30.0, "rsi_14_1h", Some(75000.0));
        let outcome = evaluate_indicator_trigger(&setup, 68000.0, Some(25.0));
        assert_eq!(outcome, TriggerOutcome::Triggered { price: 68000.0 });
    }

    #[test]
    fn indicator_below_unchanged() {
        let setup =
            make_indicator_setup("short", "indicator_below", 30.0, "rsi_14_1h", Some(75000.0));
        let outcome = evaluate_indicator_trigger(&setup, 68000.0, Some(35.0));
        assert_eq!(outcome, TriggerOutcome::Unchanged);
    }

    #[test]
    fn indicator_trigger_invalidated_by_price() {
        // Indicator hasn't fired, but price breached invalidation
        let setup =
            make_indicator_setup("long", "indicator_above", 70.0, "rsi_14_1h", Some(65000.0));
        let outcome = evaluate_indicator_trigger(&setup, 64000.0, Some(50.0));
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 64000.0 });
    }

    #[test]
    fn indicator_trigger_invalidation_priority_over_indicator() {
        // Indicator has fired AND price breached invalidation — invalidation wins
        let setup =
            make_indicator_setup("long", "indicator_above", 70.0, "rsi_14_1h", Some(65000.0));
        let outcome = evaluate_indicator_trigger(&setup, 64000.0, Some(75.0));
        assert_eq!(outcome, TriggerOutcome::Invalidated { price: 64000.0 });
    }

    #[test]
    fn missing_indicator_value_returns_skipped() {
        let setup =
            make_indicator_setup("long", "indicator_above", 70.0, "rsi_14_1h", Some(65000.0));
        let outcome = evaluate_indicator_trigger(&setup, 68000.0, None);
        assert!(matches!(outcome, TriggerOutcome::Skipped { .. }));
    }

    // --- parse_trigger_field ---

    #[test]
    fn parse_rsi_14_1h() {
        assert_eq!(parse_trigger_field("rsi_14_1h"), Some(("rsi_14", "1h")));
    }

    #[test]
    fn parse_bollinger_bandwidth_5m() {
        assert_eq!(
            parse_trigger_field("bollinger_bandwidth_5m"),
            Some(("bollinger_bandwidth", "5m"))
        );
    }

    #[test]
    fn parse_stochastic_rsi_k_1d() {
        assert_eq!(
            parse_trigger_field("stochastic_rsi_k_1d"),
            Some(("stochastic_rsi_k", "1d"))
        );
    }

    #[test]
    fn parse_historical_volatility_20_1h() {
        assert_eq!(
            parse_trigger_field("historical_volatility_20_1h"),
            Some(("historical_volatility_20", "1h"))
        );
    }

    #[test]
    fn parse_sma_20_5m() {
        assert_eq!(parse_trigger_field("sma_20_5m"), Some(("sma_20", "5m")));
    }

    #[test]
    fn parse_invalid_no_timeframe() {
        assert_eq!(parse_trigger_field("rsi_14"), None);
    }

    #[test]
    fn parse_invalid_empty_indicator() {
        assert_eq!(parse_trigger_field("_1h"), None);
    }

    #[test]
    fn parse_invalid_unknown_timeframe() {
        assert_eq!(parse_trigger_field("rsi_14_4h"), None);
    }

    // --- extract_indicator_value ---

    #[test]
    fn extract_value_present() {
        let json = serde_json::json!({
            "BTCUSDT": {
                "1h": {
                    "periods": [
                        {"rsi_14": 45.0},
                        {"rsi_14": 55.0},
                        {"rsi_14": 72.5},
                    ]
                }
            }
        });
        assert_eq!(
            extract_indicator_value(&json, "BTCUSDT", "1h", "rsi_14"),
            Some(72.5)
        );
    }

    #[test]
    fn extract_value_null() {
        let json = serde_json::json!({
            "BTCUSDT": {
                "1h": {
                    "periods": [
                        {"rsi_14": null},
                    ]
                }
            }
        });
        assert_eq!(
            extract_indicator_value(&json, "BTCUSDT", "1h", "rsi_14"),
            None
        );
    }

    #[test]
    fn extract_value_missing_asset() {
        let json = serde_json::json!({
            "ETHUSDT": {
                "1h": {
                    "periods": [{"rsi_14": 50.0}]
                }
            }
        });
        assert_eq!(
            extract_indicator_value(&json, "BTCUSDT", "1h", "rsi_14"),
            None
        );
    }

    #[test]
    fn extract_value_missing_timeframe() {
        let json = serde_json::json!({
            "BTCUSDT": {
                "5m": {
                    "periods": [{"rsi_14": 50.0}]
                }
            }
        });
        assert_eq!(
            extract_indicator_value(&json, "BTCUSDT", "1h", "rsi_14"),
            None
        );
    }

    #[test]
    fn extract_value_missing_indicator_key() {
        let json = serde_json::json!({
            "BTCUSDT": {
                "1h": {
                    "periods": [{"sma_20": 50.0}]
                }
            }
        });
        assert_eq!(
            extract_indicator_value(&json, "BTCUSDT", "1h", "rsi_14"),
            None
        );
    }

    #[test]
    fn extract_value_empty_periods() {
        let json = serde_json::json!({
            "BTCUSDT": {
                "1h": {
                    "periods": []
                }
            }
        });
        assert_eq!(
            extract_indicator_value(&json, "BTCUSDT", "1h", "rsi_14"),
            None
        );
    }
}
