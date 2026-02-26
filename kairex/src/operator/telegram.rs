use tokio::sync::broadcast;
use tracing::{debug, error};

use crate::delivery::TelegramClient;

use super::{OperatorEvent, Severity, Verbosity};

/// Format an operator event as Telegram HTML.
fn format_event(event: &OperatorEvent) -> String {
    let prefix = match event.severity() {
        Severity::Info => "[INFO]",
        Severity::Warning => "[WARN]",
        Severity::Error => "[ERROR]",
    };

    let body = html_escape(&event.to_string());
    format!("<b>{prefix}</b> {body}")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Long-lived task: receives operator events and forwards to Telegram.
///
/// Filters events by verbosity. If Telegram send fails, logs and continues
/// (operator notifications must never crash the system they monitor).
pub async fn run_subscriber(
    mut rx: broadcast::Receiver<OperatorEvent>,
    telegram: TelegramClient,
    verbosity: Verbosity,
) {
    let min_severity = verbosity.min_severity();
    debug!(?verbosity, "operator telegram subscriber started");

    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.severity() >= min_severity {
                    let html = format_event(&event);
                    if let Err(e) = telegram.send_operator(&html).await {
                        error!("operator notification send failed: {e}");
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                error!(skipped = n, "operator subscriber lagged, events dropped");
            }
            Err(broadcast::error::RecvError::Closed) => {
                debug!("operator event bus closed, subscriber exiting");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_info_event() {
        let event = OperatorEvent::SystemStarted {
            assets: vec!["BTCUSDT".into(), "ETHUSDT".into()],
        };
        let html = format_event(&event);
        assert!(html.starts_with("<b>[INFO]</b>"));
        assert!(html.contains("2 assets"));
    }

    #[test]
    fn format_warning_event() {
        let event = OperatorEvent::MetricsUnavailable {
            error: "port 9090 in use".into(),
        };
        let html = format_event(&event);
        assert!(html.starts_with("<b>[WARN]</b>"));
        assert!(html.contains("port 9090 in use"));
    }

    #[test]
    fn format_error_event() {
        let event = OperatorEvent::PipelineError {
            report_type: "morning".into(),
            stage: "analyst".into(),
            error: "rate limited".into(),
        };
        let html = format_event(&event);
        assert!(html.starts_with("<b>[ERROR]</b>"));
        assert!(html.contains("morning"));
    }

    #[test]
    fn html_special_chars_escaped() {
        let event = OperatorEvent::BackfillFailed {
            error: "timeout <5s & retries exhausted".into(),
        };
        let html = format_event(&event);
        assert!(html.contains("&lt;5s"));
        assert!(html.contains("&amp;"));
    }
}
