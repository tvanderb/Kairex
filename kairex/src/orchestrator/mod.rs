pub mod error;

pub use error::{OrchestratorError, Result};

use std::path::PathBuf;

use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::analysis;
use crate::config::AnalysisConfig;
use crate::delivery::{DeliveryLayer, Report};
use crate::evaluation::EvalEvent;
use crate::llm::{AnthropicClient, ReportType};
use crate::scheduling::ScheduleEvent;
use crate::storage::{extract_setups, Database, SystemOutput};

pub struct Orchestrator {
    db: Database,
    llm_client: AnthropicClient,
    delivery: DeliveryLayer,
    analysis_config: AnalysisConfig,
    assets: Vec<String>,
    project_root: PathBuf,
}

impl Orchestrator {
    pub fn new(
        db: Database,
        llm_client: AnthropicClient,
        delivery: DeliveryLayer,
        analysis_config: AnalysisConfig,
        assets: Vec<String>,
        project_root: PathBuf,
    ) -> Self {
        Self {
            db,
            llm_client,
            delivery,
            analysis_config,
            assets,
            project_root,
        }
    }

    /// Consume both receivers, process events forever.
    pub async fn run(
        self,
        mut schedule_rx: mpsc::Receiver<ScheduleEvent>,
        mut eval_rx: mpsc::Receiver<EvalEvent>,
    ) {
        info!("orchestrator started, waiting for events");
        loop {
            tokio::select! {
                Some(event) = schedule_rx.recv() => {
                    self.handle_schedule_event(event).await;
                }
                Some(event) = eval_rx.recv() => {
                    self.handle_eval_event(event).await;
                }
                else => {
                    info!("all event sources closed, orchestrator shutting down");
                    break;
                }
            }
        }
    }

    /// Handle a scheduled report: build full context, generate, store, deliver.
    async fn handle_schedule_event(&self, event: ScheduleEvent) {
        let ScheduleEvent::GenerateReport { report_type, .. } = event;

        let parsed = match ReportType::parse(&report_type) {
            Some(rt) => rt,
            None => {
                error!(report_type, "unknown report type from scheduler");
                return;
            }
        };

        info!(?parsed, "handling scheduled report");

        let context = match analysis::build_context(
            &self.db,
            &self.assets,
            &self.analysis_config,
            &self.project_root,
        )
        .await
        {
            Ok(ctx) => ctx,
            Err(e) => {
                error!(error = %e, ?parsed, "failed to build context for scheduled report");
                return;
            }
        };

        if let Err(e) = self.run_pipeline(parsed, &context).await {
            error!(error = %e, ?parsed, "pipeline failed for scheduled report");
        }
    }

    /// Handle an eval event: build single-asset context + trigger metadata, generate alert, store, deliver.
    async fn handle_eval_event(&self, event: EvalEvent) {
        let (setup, event_type, event_price) = match &event {
            EvalEvent::Triggered {
                setup,
                trigger_price,
                ..
            } => (setup, "triggered", *trigger_price),
            EvalEvent::Invalidated {
                setup,
                invalidation_price,
                ..
            } => (setup, "invalidated", *invalidation_price),
        };

        info!(
            asset = %setup.asset,
            event_type,
            event_price,
            "handling eval event"
        );

        let asset_list = vec![setup.asset.clone()];
        let mut context = match analysis::build_context(
            &self.db,
            &asset_list,
            &self.analysis_config,
            &self.project_root,
        )
        .await
        {
            Ok(ctx) => ctx,
            Err(e) => {
                error!(error = %e, asset = %setup.asset, "failed to build context for alert");
                return;
            }
        };

        // Augment context with trigger metadata
        if let Some(obj) = context.as_object_mut() {
            obj.insert(
                "alert_trigger".to_string(),
                json!({
                    "event_type": event_type,
                    "asset": setup.asset,
                    "direction": setup.direction,
                    "trigger_condition": setup.trigger_condition,
                    "trigger_level": setup.trigger_level,
                    "trigger_field": setup.trigger_field,
                    "invalidation_level": setup.invalidation_level,
                    "event_price": event_price,
                    "confidence": setup.confidence,
                }),
            );
        }

        if let Err(e) = self.run_pipeline(ReportType::Alert, &context).await {
            error!(error = %e, asset = %setup.asset, "pipeline failed for alert");
        }
    }

    /// Shared pipeline: LLM generate, store in DB, extract setups, deserialize, deliver.
    async fn run_pipeline(
        &self,
        report_type: ReportType,
        context: &serde_json::Value,
    ) -> Result<()> {
        let llm_response = self
            .llm_client
            .generate(report_type, context, &self.project_root)
            .await?;

        info!(
            ?report_type,
            input_tokens = llm_response.input_tokens,
            output_tokens = llm_response.output_tokens,
            "LLM generation complete"
        );

        let now = now_ms();

        // Store raw output
        let output = SystemOutput {
            id: None,
            report_type: report_type
                .tool_name()
                .trim_end_matches("_report")
                .to_string(),
            generated_at: now,
            schema_version: "v1".to_string(),
            output: llm_response.output.clone(),
            delivered_at: None,
            delivery_status: "pending".to_string(),
        };

        let setups = extract_setups(&llm_response.output, 0, now);
        let output_id = self.db.store_report(&output, &setups)?;

        info!(
            ?report_type,
            output_id,
            setups = setups.len(),
            "stored report and setups"
        );

        // Deserialize into typed Report for delivery
        let report = deserialize_report(report_type, &llm_response.output)?;

        self.delivery.deliver(&report, output_id).await?;

        info!(?report_type, output_id, "delivery complete");

        Ok(())
    }
}

/// Deserialize raw LLM JSON output into the typed Report enum for delivery.
fn deserialize_report(report_type: ReportType, output: &serde_json::Value) -> Result<Report> {
    use crate::llm::LlmError;
    fn parse<T: serde::de::DeserializeOwned>(
        v: serde_json::Value,
    ) -> std::result::Result<T, LlmError> {
        serde_json::from_value(v).map_err(LlmError::from)
    }
    let report = match report_type {
        ReportType::Morning => Report::Morning(parse(output.clone())?),
        ReportType::Midday => Report::Midday(parse(output.clone())?),
        ReportType::Evening => Report::Evening(parse(output.clone())?),
        ReportType::Alert => Report::Alert(parse(output.clone())?),
        ReportType::Weekly => Report::Weekly(parse(output.clone())?),
    };
    Ok(report)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
