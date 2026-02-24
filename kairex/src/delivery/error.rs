use thiserror::Error;

pub type Result<T> = std::result::Result<T, DeliveryError>;

#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Telegram API error: {status} — {description}")]
    TelegramApi { status: u16, description: String },

    #[error("config error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("missing environment variable: {0}")]
    MissingEnvVar(String),
}
