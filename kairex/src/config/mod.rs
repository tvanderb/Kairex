mod analysis;
mod assets;
mod collection;
mod delivery;
mod evaluation;
mod llm;
mod operator;
mod schedules;

pub use analysis::{AnalysisConfig, IndicatorsConfig};
pub use assets::{Asset, AssetsConfig};
pub use collection::{CollectionConfig, PollEndpoint, PollingConfig, RetryConfig, WebSocketConfig};
pub use delivery::{
    DeliveryConfig, FormatMode, FreeChannelConfig, RouteConfig, RouteMode, RouteRule, SetupFormat,
};
pub use evaluation::{CooldownConfig, EvaluationConfig};
pub use llm::{LlmConfig, LlmRetryConfig};
pub use operator::OperatorConfig;
pub use schedules::{OvernightConfig, ReportSchedule, SchedulesConfig, WeeklySchedule};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
