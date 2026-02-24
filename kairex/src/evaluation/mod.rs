pub mod error;
pub mod trigger;

pub use error::{EvaluationError, Result};
pub use trigger::TriggerOutcome;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

use crate::analysis;
use crate::config::{AnalysisConfig, EvaluationConfig};
use crate::storage::{ActiveSetup, Database, FiredAlert};

use trigger::{
    evaluate_indicator_trigger, evaluate_price_trigger, extract_indicator_value,
    parse_trigger_field,
};

/// Events emitted by the evaluation layer.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalEvent {
    /// A setup's trigger condition was met.
    Triggered {
        setup: ActiveSetup,
        trigger_price: f64,
        timestamp: i64,
    },
    /// A setup was invalidated (price breached invalidation level).
    Invalidated {
        setup: ActiveSetup,
        invalidation_price: f64,
        timestamp: i64,
    },
}

/// The evaluation layer: periodically checks active setups against live data.
pub struct EvaluationLayer {
    db: Database,
    eval_config: EvaluationConfig,
    analysis_config: AnalysisConfig,
    project_root: PathBuf,
}

impl EvaluationLayer {
    pub fn new(
        db: Database,
        eval_config: EvaluationConfig,
        analysis_config: AnalysisConfig,
        project_root: PathBuf,
    ) -> Self {
        Self {
            db,
            eval_config,
            analysis_config,
            project_root,
        }
    }

    /// Start the evaluation loop. Returns a receiver for evaluation events.
    pub fn start(self) -> mpsc::Receiver<EvalEvent> {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            self.run_loop(tx).await;
        });
        rx
    }

    async fn run_loop(self, tx: mpsc::Sender<EvalEvent>) {
        let interval = Duration::from_secs(self.eval_config.cycle_interval_seconds);
        loop {
            match self.run_cycle(&tx).await {
                Ok(count) => {
                    if count > 0 {
                        info!(events = count, "evaluation cycle complete");
                        metrics::counter!("kairex_eval_cycles_total", "outcome" => "events_emitted")
                            .increment(1);
                    } else {
                        debug!("evaluation cycle complete, no events");
                        metrics::counter!("kairex_eval_cycles_total", "outcome" => "no_events")
                            .increment(1);
                    }
                }
                Err(e) => {
                    warn!(error = %e, "evaluation cycle failed");
                    metrics::counter!("kairex_eval_cycles_total", "outcome" => "error")
                        .increment(1);
                }
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Run one evaluation cycle. Returns the number of events emitted.
    #[instrument(name = "evaluation.cycle", skip_all)]
    pub async fn run_cycle(&self, tx: &mpsc::Sender<EvalEvent>) -> Result<usize> {
        let setups = db_blocking(&self.db, |db| db.query_active_setups()).await?;
        if setups.is_empty() {
            return Ok(0);
        }

        // Partition into price vs indicator setups
        let (price_setups, indicator_setups): (Vec<_>, Vec<_>) = setups
            .into_iter()
            .partition(|s| is_price_trigger(&s.trigger_condition));

        let now = now_ms();
        let mut event_count = 0;

        // --- Price setups: fetch latest 5m candle per asset ---
        let price_assets: HashSet<String> = price_setups.iter().map(|s| s.asset.clone()).collect();

        let mut latest_prices: HashMap<String, f64> = HashMap::new();
        for asset in &price_assets {
            let asset_owned = asset.clone();
            match db_blocking(&self.db, move |db| {
                db.query_latest_candle(&asset_owned, "5m")
            })
            .await?
            {
                Some(candle) => {
                    latest_prices.insert(asset.clone(), candle.close);
                }
                None => {
                    debug!(asset = %asset, "no 5m candle found, skipping price setups for asset");
                }
            }
        }

        for setup in &price_setups {
            if let Some(&close) = latest_prices.get(&setup.asset) {
                let outcome = evaluate_price_trigger(setup, close);
                event_count += self.handle_outcome(setup, outcome, now, tx).await?;
            }
        }

        // --- Indicator setups: compute indicators for relevant assets ---
        if !indicator_setups.is_empty() {
            let indicator_assets: Vec<String> = indicator_setups
                .iter()
                .map(|s| s.asset.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            // Also need latest prices for invalidation checks
            for asset in &indicator_assets {
                if !latest_prices.contains_key(asset) {
                    let asset_owned = asset.clone();
                    if let Some(candle) = db_blocking(&self.db, move |db| {
                        db.query_latest_candle(&asset_owned, "5m")
                    })
                    .await?
                    {
                        latest_prices.insert(asset.clone(), candle.close);
                    }
                }
            }

            match analysis::compute_indicators(
                &self.db,
                &indicator_assets,
                &self.analysis_config,
                &self.project_root,
            )
            .await
            {
                Ok(indicators_json) => {
                    for setup in &indicator_setups {
                        let close = match latest_prices.get(&setup.asset) {
                            Some(&p) => p,
                            None => {
                                debug!(asset = %setup.asset, "no price for indicator setup, skipping");
                                continue;
                            }
                        };

                        let indicator_value = setup
                            .trigger_field
                            .as_deref()
                            .and_then(parse_trigger_field)
                            .and_then(|(indicator, timeframe)| {
                                extract_indicator_value(
                                    &indicators_json,
                                    &setup.asset,
                                    timeframe,
                                    indicator,
                                )
                            });

                        let outcome = evaluate_indicator_trigger(setup, close, indicator_value);
                        event_count += self.handle_outcome(setup, outcome, now, tx).await?;
                    }
                }
                Err(e) => {
                    warn!(error = %e, "indicator computation failed, skipping indicator setups");
                }
            }
        }

        Ok(event_count)
    }

    /// Handle a trigger outcome: resolve in DB, check cooldown, record fired alert, emit event.
    /// Returns 1 if an event was emitted, 0 otherwise.
    async fn handle_outcome(
        &self,
        setup: &ActiveSetup,
        outcome: TriggerOutcome,
        now: i64,
        tx: &mpsc::Sender<EvalEvent>,
    ) -> Result<usize> {
        let setup_id = match setup.id {
            Some(id) => id,
            None => return Ok(0),
        };

        match outcome {
            TriggerOutcome::Triggered { price } => {
                // Always resolve in DB
                db_blocking(&self.db, move |db| {
                    db.resolve_setup(setup_id, "triggered", now, price)
                })
                .await?;

                // Check cooldown before emitting event
                let alert_type = format!("setup_trigger:{}", setup_id);
                let on_cooldown = db_blocking(&self.db, {
                    let alert_type = alert_type.clone();
                    move |db| db.is_alert_on_cooldown(&alert_type, now)
                })
                .await?;

                if on_cooldown {
                    debug!(setup_id, "trigger suppressed by cooldown");
                    return Ok(0);
                }

                // Record fired alert
                let cooldown_until =
                    now + (self.eval_config.cooldown_minutes.setup_trigger as i64) * 60 * 1000;
                let alert = FiredAlert {
                    id: None,
                    setup_id: Some(setup_id),
                    alert_type: alert_type.clone(),
                    fired_at: now,
                    cooldown_until,
                    output_id: None,
                };
                db_blocking(&self.db, move |db| db.insert_fired_alert(&alert)).await?;

                // Emit event
                let event = EvalEvent::Triggered {
                    setup: setup.clone(),
                    trigger_price: price,
                    timestamp: now,
                };
                let _ = tx.send(event).await;
                Ok(1)
            }
            TriggerOutcome::Invalidated { price } => {
                // Always resolve in DB
                db_blocking(&self.db, move |db| {
                    db.resolve_setup(setup_id, "invalidated", now, price)
                })
                .await?;

                // Check cooldown
                let alert_type = format!("setup_invalidation:{}", setup_id);
                let on_cooldown = db_blocking(&self.db, {
                    let alert_type = alert_type.clone();
                    move |db| db.is_alert_on_cooldown(&alert_type, now)
                })
                .await?;

                if on_cooldown {
                    debug!(setup_id, "invalidation suppressed by cooldown");
                    return Ok(0);
                }

                let cooldown_until =
                    now + (self.eval_config.cooldown_minutes.setup_invalidation as i64) * 60 * 1000;
                let alert = FiredAlert {
                    id: None,
                    setup_id: Some(setup_id),
                    alert_type: alert_type.clone(),
                    fired_at: now,
                    cooldown_until,
                    output_id: None,
                };
                db_blocking(&self.db, move |db| db.insert_fired_alert(&alert)).await?;

                let event = EvalEvent::Invalidated {
                    setup: setup.clone(),
                    invalidation_price: price,
                    timestamp: now,
                };
                let _ = tx.send(event).await;
                Ok(1)
            }
            TriggerOutcome::Unchanged | TriggerOutcome::Skipped { .. } => Ok(0),
        }
    }
}

fn is_price_trigger(condition: &str) -> bool {
    matches!(condition, "price_above" | "price_below")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Async-safe bridge: run a synchronous Database closure on a blocking thread.
async fn db_blocking<F, T>(db: &Database, f: F) -> Result<T>
where
    F: FnOnce(&Database) -> crate::storage::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || f(&db).map_err(EvaluationError::Storage)).await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CooldownConfig;
    use crate::storage::{Candle, Database, SystemOutput};

    fn test_db() -> (tempfile::TempDir, Database) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory(tmp.path()).unwrap();
        (tmp, db)
    }

    fn test_eval_config() -> EvaluationConfig {
        EvaluationConfig {
            cycle_interval_seconds: 60,
            cooldown_minutes: CooldownConfig {
                setup_trigger: 60,
                setup_invalidation: 60,
            },
        }
    }

    fn test_analysis_config() -> AnalysisConfig {
        use crate::config::IndicatorsConfig;
        AnalysisConfig {
            indicators: IndicatorsConfig {
                context_periods: 17,
                compute_timeout_seconds: 30,
                context_timeout_seconds: 60,
                python_venv: ".venv".into(),
            },
        }
    }

    fn make_layer(db: Database) -> EvaluationLayer {
        EvaluationLayer::new(
            db,
            test_eval_config(),
            test_analysis_config(),
            PathBuf::from("."),
        )
    }

    fn insert_candle(db: &Database, symbol: &str, close: f64) {
        db.insert_candle(&Candle {
            symbol: symbol.into(),
            timeframe: "5m".into(),
            open_time: 1000,
            open: close,
            high: close + 100.0,
            low: close - 100.0,
            close,
            volume: 1000.0,
            source: "test".into(),
        })
        .unwrap();
    }

    fn insert_setup(
        db: &Database,
        asset: &str,
        trigger_condition: &str,
        trigger_level: f64,
        direction: &str,
        invalidation_level: Option<f64>,
    ) -> i64 {
        let output = SystemOutput {
            id: None,
            report_type: "morning".into(),
            generated_at: 1000,
            schema_version: "v1".into(),
            output: serde_json::json!({"test": true}),
            delivered_at: None,
            delivery_status: "pending".into(),
        };
        let setup = ActiveSetup {
            id: None,
            source_output_id: 0,
            asset: asset.into(),
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
        };
        db.store_report(&output, &[setup]).unwrap();
        let active = db.query_active_setups().unwrap();
        active.last().unwrap().id.unwrap()
    }

    #[tokio::test]
    async fn no_active_setups_emits_zero_events() {
        let (_tmp, db) = test_db();
        let layer = make_layer(db);
        let (tx, mut rx) = mpsc::channel(16);

        let count = layer.run_cycle(&tx).await.unwrap();
        assert_eq!(count, 0);

        // Channel should be empty
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn price_trigger_resolves_and_emits() {
        let (_tmp, db) = test_db();
        insert_candle(&db, "BTCUSDT", 71000.0);
        insert_setup(
            &db,
            "BTCUSDT",
            "price_above",
            70000.0,
            "long",
            Some(65000.0),
        );

        let layer = make_layer(db.clone());
        let (tx, mut rx) = mpsc::channel(16);

        let count = layer.run_cycle(&tx).await.unwrap();
        assert_eq!(count, 1);

        let event = rx.try_recv().unwrap();
        match event {
            EvalEvent::Triggered {
                setup,
                trigger_price,
                ..
            } => {
                assert_eq!(setup.asset, "BTCUSDT");
                assert_eq!(trigger_price, 71000.0);
            }
            _ => panic!("expected Triggered event"),
        }

        // Setup should be resolved in DB
        let active = db.query_active_setups().unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn invalidation_resolves_and_emits() {
        let (_tmp, db) = test_db();
        insert_candle(&db, "BTCUSDT", 64000.0);
        insert_setup(
            &db,
            "BTCUSDT",
            "price_above",
            70000.0,
            "long",
            Some(65000.0),
        );

        let layer = make_layer(db.clone());
        let (tx, mut rx) = mpsc::channel(16);

        let count = layer.run_cycle(&tx).await.unwrap();
        assert_eq!(count, 1);

        let event = rx.try_recv().unwrap();
        match event {
            EvalEvent::Invalidated {
                setup,
                invalidation_price,
                ..
            } => {
                assert_eq!(setup.asset, "BTCUSDT");
                assert_eq!(invalidation_price, 64000.0);
            }
            _ => panic!("expected Invalidated event"),
        }

        let active = db.query_active_setups().unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn cooldown_suppresses_event_but_resolves_setup() {
        let (_tmp, db) = test_db();
        insert_candle(&db, "BTCUSDT", 71000.0);
        let setup_id = insert_setup(
            &db,
            "BTCUSDT",
            "price_above",
            70000.0,
            "long",
            Some(65000.0),
        );

        // Pre-insert a fired alert that's still on cooldown (far future cooldown_until)
        let alert_type = format!("setup_trigger:{}", setup_id);
        let alert = FiredAlert {
            id: None,
            setup_id: Some(setup_id),
            alert_type,
            fired_at: 500,
            cooldown_until: i64::MAX, // never expires in this test
            output_id: None,
        };
        db.insert_fired_alert(&alert).unwrap();

        let layer = make_layer(db.clone());
        let (tx, mut rx) = mpsc::channel(16);

        let count = layer.run_cycle(&tx).await.unwrap();
        assert_eq!(count, 0); // event suppressed

        // But setup is still resolved in DB
        let active = db.query_active_setups().unwrap();
        assert!(active.is_empty());

        // No event on channel
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn multiple_assets_in_one_cycle() {
        let (_tmp, db) = test_db();
        insert_candle(&db, "BTCUSDT", 71000.0);
        insert_candle(&db, "ETHUSDT", 1800.0);

        // Insert both setups in a single report so neither gets expired
        let output = SystemOutput {
            id: None,
            report_type: "morning".into(),
            generated_at: 1000,
            schema_version: "v1".into(),
            output: serde_json::json!({"test": true}),
            delivered_at: None,
            delivery_status: "pending".into(),
        };
        let setups = vec![
            ActiveSetup {
                id: None,
                source_output_id: 0,
                asset: "BTCUSDT".into(),
                direction: "long".into(),
                trigger_condition: "price_above".into(),
                trigger_level: 70000.0,
                trigger_field: None,
                target_level: Some(75000.0),
                invalidation_level: Some(65000.0),
                confidence: Some(0.7),
                status: "active".into(),
                created_at: 1000,
                resolved_at: None,
                resolved_price: None,
            },
            ActiveSetup {
                id: None,
                source_output_id: 0,
                asset: "ETHUSDT".into(),
                direction: "long".into(),
                trigger_condition: "price_above".into(),
                trigger_level: 2000.0,
                trigger_field: None,
                target_level: Some(2200.0),
                invalidation_level: Some(1850.0),
                confidence: Some(0.6),
                status: "active".into(),
                created_at: 1000,
                resolved_at: None,
                resolved_price: None,
            },
        ];
        db.store_report(&output, &setups).unwrap();

        let layer = make_layer(db.clone());
        let (tx, mut rx) = mpsc::channel(16);

        let count = layer.run_cycle(&tx).await.unwrap();
        assert_eq!(count, 2);

        let mut triggered = false;
        let mut invalidated = false;
        for _ in 0..2 {
            match rx.try_recv().unwrap() {
                EvalEvent::Triggered { setup, .. } => {
                    assert_eq!(setup.asset, "BTCUSDT");
                    triggered = true;
                }
                EvalEvent::Invalidated { setup, .. } => {
                    assert_eq!(setup.asset, "ETHUSDT");
                    invalidated = true;
                }
            }
        }
        assert!(triggered);
        assert!(invalidated);

        let active = db.query_active_setups().unwrap();
        assert!(active.is_empty());
    }
}
