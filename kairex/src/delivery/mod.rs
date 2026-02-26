mod error;
pub mod format;
mod routing;
mod telegram;

pub use error::{DeliveryError, Result};
pub use routing::{FreeChannelRouter, RouteDecision};
pub use telegram::TelegramClient;

use tracing::{error, info, instrument};

use crate::config::{DeliveryConfig, FormatMode, FreeChannelConfig, SetupFormat};
use crate::llm::schemas::{AlertReport, EveningReport, MiddayReport, MorningReport, WeeklyReport};
use crate::llm::types::Significance;
use crate::llm::{EditorOutput, ReportType};
use crate::operator::{OperatorEvent, OperatorSender};
use crate::storage::Database;

/// Wraps all 5 report types for unified delivery handling.
pub enum Report {
    Morning(MorningReport),
    Midday(MiddayReport),
    Evening(EveningReport),
    Alert(AlertReport),
    Weekly(WeeklyReport),
}

impl Report {
    pub fn significance(&self) -> &Significance {
        match self {
            Self::Morning(r) => &r.significance,
            Self::Midday(r) => &r.significance,
            Self::Evening(r) => &r.significance,
            Self::Alert(r) => &r.significance,
            Self::Weekly(r) => &r.significance,
        }
    }

    pub fn report_type(&self) -> ReportType {
        match self {
            Self::Morning(_) => ReportType::Morning,
            Self::Midday(_) => ReportType::Midday,
            Self::Evening(_) => ReportType::Evening,
            Self::Alert(_) => ReportType::Alert,
            Self::Weekly(_) => ReportType::Weekly,
        }
    }
}

pub struct DeliveryLayer {
    telegram: TelegramClient,
    router: FreeChannelRouter,
    setup_format: SetupFormat,
    db: Database,
    operator: OperatorSender,
}

impl DeliveryLayer {
    pub fn new(
        telegram: TelegramClient,
        delivery_config: &DeliveryConfig,
        free_channel_config: FreeChannelConfig,
        db: Database,
        operator: OperatorSender,
    ) -> Self {
        let router = FreeChannelRouter::new(free_channel_config);

        Self {
            telegram,
            router,
            setup_format: delivery_config.setup_format,
            db,
            operator,
        }
    }

    /// Deliver a report to premium channel, optionally to free channel, and update DB.
    #[instrument(name = "delivery.deliver", skip(self, report), fields(report_type = ?report.report_type(), output_id))]
    pub async fn deliver(&self, report: &Report, output_id: i64) -> Result<()> {
        let delivery_start = std::time::Instant::now();
        let sf = self.setup_format;
        let report_type = report.report_type();
        let now_ms = now_millis();

        // Format for premium
        let premium_html = format_for_premium(report, sf);

        // Send to premium
        if let Err(e) = self.telegram.send_premium(&premium_html).await {
            error!(?report_type, "failed to send to premium channel: {e}");
            self.update_status(output_id, "failed", now_ms);
            self.operator.emit(OperatorEvent::DeliveryFailed {
                destination: "premium".into(),
                error: e.to_string(),
            });
            return Err(e);
        }

        info!(?report_type, "delivered to premium channel");

        // Evaluate free channel routing
        let decision = self.router.evaluate(report_type, report.significance());

        match decision {
            RouteDecision::Send(FormatMode::PassThrough) => {
                if let Err(e) = self.telegram.send_free(&premium_html).await {
                    error!(?report_type, "failed to send to free channel: {e}");
                    self.operator.emit(OperatorEvent::DeliveryFailed {
                        destination: "free".into(),
                        error: e.to_string(),
                    });
                }
            }
            RouteDecision::Send(FormatMode::Condensed) => {
                let condensed_html = format_for_free(report, sf);
                if let Err(e) = self.telegram.send_free(&condensed_html).await {
                    error!(
                        ?report_type,
                        "failed to send condensed to free channel: {e}"
                    );
                    self.operator.emit(OperatorEvent::DeliveryFailed {
                        destination: "free".into(),
                        error: e.to_string(),
                    });
                }
            }
            RouteDecision::Skip => {}
        }

        self.update_status(output_id, "delivered", now_ms);

        let type_label = report_type
            .tool_name()
            .trim_end_matches("_report")
            .to_string();
        metrics::histogram!("kairex_delivery_duration_seconds", "report_type" => type_label)
            .record(delivery_start.elapsed().as_secs_f64());

        Ok(())
    }

    /// Expose routing decision for orchestrator (called before editor).
    pub fn evaluate_route(
        &self,
        report_type: ReportType,
        significance: &Significance,
    ) -> RouteDecision {
        self.router.evaluate(report_type, significance)
    }

    /// Deliver pre-formatted editor output to Telegram.
    #[instrument(
        name = "delivery.deliver_edited",
        skip(self, editor_output, route_decision),
        fields(output_id)
    )]
    pub async fn deliver_edited(
        &self,
        editor_output: &EditorOutput,
        route_decision: &RouteDecision,
        output_id: i64,
    ) -> Result<()> {
        let delivery_start = std::time::Instant::now();
        let now_ms = now_millis();

        // Send to premium
        if let Err(e) = self
            .telegram
            .send_premium(&editor_output.premium_html)
            .await
        {
            error!("failed to send editor output to premium channel: {e}");
            self.update_status(output_id, "failed", now_ms);
            self.operator.emit(OperatorEvent::DeliveryFailed {
                destination: "premium".into(),
                error: e.to_string(),
            });
            return Err(e);
        }

        info!("delivered editor output to premium channel");

        // Send to free if routed and editor produced a free version
        match route_decision {
            RouteDecision::Send(_) => {
                if let Some(ref free_html) = editor_output.free_html {
                    if let Err(e) = self.telegram.send_free(free_html).await {
                        error!("failed to send editor output to free channel: {e}");
                        self.operator.emit(OperatorEvent::DeliveryFailed {
                            destination: "free".into(),
                            error: e.to_string(),
                        });
                    }
                }
            }
            RouteDecision::Skip => {}
        }

        self.update_status(output_id, "delivered", now_ms);

        metrics::histogram!("kairex_delivery_duration_seconds", "stage" => "editor")
            .record(delivery_start.elapsed().as_secs_f64());

        Ok(())
    }

    fn update_status(&self, output_id: i64, status: &str, delivered_at: i64) {
        if let Err(e) = self
            .db
            .update_delivery_status(output_id, status, delivered_at)
        {
            error!(output_id, "failed to update delivery status: {e}");
        }
    }
}

fn format_for_premium(report: &Report, sf: SetupFormat) -> String {
    match report {
        Report::Morning(r) => format::format_morning(r, sf),
        Report::Midday(r) => format::format_midday(r, sf),
        Report::Evening(r) => format::format_evening(r, sf),
        Report::Alert(r) => format::format_alert(r, sf),
        Report::Weekly(r) => format::format_weekly(r, sf),
    }
}

fn format_for_free(report: &Report, sf: SetupFormat) -> String {
    match report {
        Report::Morning(r) => format::format_morning_condensed(r, sf),
        Report::Evening(r) => format::format_evening_condensed(r, sf),
        Report::Alert(r) => format::format_alert_condensed(r, sf),
        // Weekly uses pass_through (handled by caller), these shouldn't be called
        // but produce full format as fallback
        Report::Weekly(r) => format::format_weekly(r, sf),
        Report::Midday(r) => format::format_midday(r, sf),
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
        let path = format!(
            "{}/tests/fixtures/llm/{name}",
            env!("CARGO_MANIFEST_DIR").trim_end_matches("/kairex")
        );
        let json =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("failed to parse {path}: {e}"))
    }

    #[test]
    fn report_enum_significance_accessor() {
        let morning: MorningReport = load_fixture("morning_report.json");
        let sig = morning.significance.clone();
        let report = Report::Morning(morning);
        assert_eq!(report.significance(), &sig);
        assert_eq!(report.report_type(), ReportType::Morning);
    }

    #[test]
    fn report_enum_all_variants() {
        let morning: MorningReport = load_fixture("morning_report.json");
        assert_eq!(Report::Morning(morning).report_type(), ReportType::Morning);

        let midday: MiddayReport = load_fixture("midday_report.json");
        assert_eq!(Report::Midday(midday).report_type(), ReportType::Midday);

        let evening: EveningReport = load_fixture("evening_report.json");
        assert_eq!(Report::Evening(evening).report_type(), ReportType::Evening);

        let alert: AlertReport = load_fixture("alert_report.json");
        assert_eq!(Report::Alert(alert).report_type(), ReportType::Alert);

        let weekly: WeeklyReport = load_fixture("weekly_report.json");
        assert_eq!(Report::Weekly(weekly).report_type(), ReportType::Weekly);
    }

    #[test]
    fn evaluate_route_delegates_to_router() {
        use crate::config::FreeChannelConfig;
        use std::path::PathBuf;

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tests/fixtures/config/free_channel.toml");
        let free_config = FreeChannelConfig::load(&path).unwrap();
        let router = FreeChannelRouter::new(free_config.clone());

        let sig = Significance {
            magnitude: 0.8,
            surprise: 0.1,
            regime_relevance: 0.1,
        };
        let expected = router.evaluate(ReportType::Morning, &sig);
        assert_eq!(expected, RouteDecision::Send(FormatMode::Condensed));

        let expected = router.evaluate(ReportType::Midday, &sig);
        assert_eq!(expected, RouteDecision::Skip);
    }

    #[test]
    fn format_premium_dispatches_all_types() {
        let sf = SetupFormat::DetailLine;

        let morning: MorningReport = load_fixture("morning_report.json");
        assert!(!format_for_premium(&Report::Morning(morning), sf).is_empty());

        let midday: MiddayReport = load_fixture("midday_report.json");
        assert!(!format_for_premium(&Report::Midday(midday), sf).is_empty());

        let evening: EveningReport = load_fixture("evening_report.json");
        assert!(!format_for_premium(&Report::Evening(evening), sf).is_empty());

        let alert: AlertReport = load_fixture("alert_report.json");
        assert!(!format_for_premium(&Report::Alert(alert), sf).is_empty());

        let weekly: WeeklyReport = load_fixture("weekly_report.json");
        assert!(!format_for_premium(&Report::Weekly(weekly), sf).is_empty());
    }
}
