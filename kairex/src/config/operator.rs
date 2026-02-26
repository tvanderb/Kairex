use std::path::Path;

use serde::Deserialize;

use crate::operator::Verbosity;

use super::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct OperatorConfig {
    #[serde(default)]
    pub verbosity: Verbosity,
}

impl OperatorConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}
