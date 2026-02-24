use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("indicator computation failed: {0}")]
    Indicator(#[from] crate::analysis::AnalysisError),

    #[error("config error: {0}")]
    Config(String),

    #[error("invalid trigger field: {0}")]
    InvalidTriggerField(String),

    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, EvaluationError>;
