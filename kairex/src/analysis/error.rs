use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("subprocess failed: {0}")]
    Subprocess(String),

    #[error("subprocess timed out after {0}s")]
    Timeout(u64),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("config error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, AnalysisError>;
