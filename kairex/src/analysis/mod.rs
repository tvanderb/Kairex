pub mod context;
pub mod error;
pub mod indicators;
pub mod subprocess;

pub use context::build_context;
pub use error::{AnalysisError, Result};
pub use indicators::compute_indicators;
