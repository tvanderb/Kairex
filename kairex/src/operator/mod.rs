pub mod telegram;

use std::fmt;

use tokio::sync::broadcast;

/// Severity level for operator events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Controls which events a subscriber receives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    /// All events (backfill, pipeline stats, reconnections, warnings, errors).
    #[default]
    Verbose,
    /// Warnings and errors only.
    Normal,
    /// Errors only.
    Quiet,
}

impl Verbosity {
    pub fn min_severity(self) -> Severity {
        match self {
            Self::Verbose => Severity::Info,
            Self::Normal => Severity::Warning,
            Self::Quiet => Severity::Error,
        }
    }
}

/// Structured operator events emitted by system components.
#[derive(Debug, Clone)]
pub enum OperatorEvent {
    // --- System lifecycle ---
    SystemStarted {
        assets: Vec<String>,
    },

    // --- Degraded operation ---
    MetricsUnavailable {
        error: String,
    },
    TracerUnavailable {
        reason: String,
    },

    // --- Collection ---
    BackfillComplete {
        candles: u64,
        funding: u64,
        open_interest: u64,
        indices: u64,
    },
    BackfillFailed {
        error: String,
    },
    WebSocketConnected {
        symbols: usize,
    },

    // --- Pipeline ---
    PipelineComplete {
        report_type: String,
        analyst_tokens: u32,
        editor_tokens: u32,
        duration_secs: f64,
        premium_chars: usize,
    },
    PipelineError {
        report_type: String,
        stage: String,
        error: String,
    },

    // --- Delivery ---
    DeliveryFailed {
        destination: String,
        error: String,
    },
}

impl OperatorEvent {
    pub fn severity(&self) -> Severity {
        match self {
            Self::SystemStarted { .. }
            | Self::BackfillComplete { .. }
            | Self::WebSocketConnected { .. }
            | Self::PipelineComplete { .. } => Severity::Info,

            Self::MetricsUnavailable { .. }
            | Self::TracerUnavailable { .. }
            | Self::BackfillFailed { .. } => Severity::Warning,

            Self::PipelineError { .. } | Self::DeliveryFailed { .. } => Severity::Error,
        }
    }
}

impl fmt::Display for OperatorEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemStarted { assets } => {
                write!(f, "System started — tracking {} assets", assets.len())
            }
            Self::MetricsUnavailable { error } => {
                write!(f, "Metrics unavailable — {error}")
            }
            Self::TracerUnavailable { reason } => {
                write!(f, "Tracer unavailable — {reason}")
            }
            Self::BackfillComplete {
                candles,
                funding,
                open_interest,
                indices,
            } => {
                write!(
                    f,
                    "Backfill complete — {candles} candles, {funding} funding, {open_interest} OI, {indices} indices"
                )
            }
            Self::BackfillFailed { error } => {
                write!(f, "Backfill failed — {error}")
            }
            Self::WebSocketConnected { symbols } => {
                write!(f, "WebSocket connected — {symbols} symbols")
            }
            Self::PipelineComplete {
                report_type,
                analyst_tokens,
                editor_tokens,
                duration_secs,
                premium_chars,
            } => {
                write!(
                    f,
                    "{report_type} complete — {analyst_tokens}+{editor_tokens} tokens, {premium_chars} chars, {duration_secs:.1}s"
                )
            }
            Self::PipelineError {
                report_type,
                stage,
                error,
            } => {
                write!(f, "{report_type} failed at {stage} — {error}")
            }
            Self::DeliveryFailed { destination, error } => {
                write!(f, "Delivery to {destination} failed — {error}")
            }
        }
    }
}

/// The event bus. Created once in main, cloned senders passed to components.
pub struct OperatorBus {
    tx: broadcast::Sender<OperatorEvent>,
}

impl OperatorBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn sender(&self) -> OperatorSender {
        OperatorSender {
            tx: self.tx.clone(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OperatorEvent> {
        self.tx.subscribe()
    }
}

/// Cheap, cloneable handle for emitting events. Passed to components.
#[derive(Clone)]
pub struct OperatorSender {
    tx: broadcast::Sender<OperatorEvent>,
}

impl OperatorSender {
    /// Emit an event. If no subscribers are listening, the event is silently dropped.
    pub fn emit(&self, event: OperatorEvent) {
        let _ = self.tx.send(event);
    }

    /// Create a no-op sender for testing (events go nowhere).
    pub fn noop() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self { tx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn verbosity_min_severity() {
        assert_eq!(Verbosity::Verbose.min_severity(), Severity::Info);
        assert_eq!(Verbosity::Normal.min_severity(), Severity::Warning);
        assert_eq!(Verbosity::Quiet.min_severity(), Severity::Error);
    }

    #[test]
    fn noop_sender_does_not_panic() {
        let sender = OperatorSender::noop();
        sender.emit(OperatorEvent::SystemStarted {
            assets: vec!["BTC".into()],
        });
    }

    #[test]
    fn bus_sends_to_subscriber() {
        let bus = OperatorBus::new(16);
        let mut rx = bus.subscribe();
        let sender = bus.sender();

        sender.emit(OperatorEvent::MetricsUnavailable {
            error: "port in use".into(),
        });

        let event = rx.try_recv().unwrap();
        assert_eq!(event.severity(), Severity::Warning);
    }

    #[test]
    fn display_formats_events() {
        let event = OperatorEvent::PipelineComplete {
            report_type: "morning".into(),
            analyst_tokens: 5000,
            editor_tokens: 400,
            duration_secs: 12.3,
            premium_chars: 800,
        };
        let s = event.to_string();
        assert!(s.contains("morning"));
        assert!(s.contains("5000+400"));
        assert!(s.contains("800 chars"));
    }
}
