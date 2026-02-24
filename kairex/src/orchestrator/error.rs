use thiserror::Error;

pub type Result<T> = std::result::Result<T, OrchestratorError>;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("analysis error: {0}")]
    Analysis(#[from] crate::analysis::AnalysisError),

    #[error("LLM error: {0}")]
    Llm(#[from] crate::llm::LlmError),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),

    #[error("delivery error: {0}")]
    Delivery(#[from] crate::delivery::DeliveryError),

    #[error("config error: {0}")]
    Config(String),
}
