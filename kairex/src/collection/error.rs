use thiserror::Error;

pub type Result<T> = std::result::Result<T, CollectionError>;

#[derive(Debug, Error)]
pub enum CollectionError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("API error: {message}")]
    Api { message: String },

    #[error("config error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}
