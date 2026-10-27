use serde::{Deserialize, Serialize};

use super::HistoryError;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct HistoryScopeId(String);

impl HistoryScopeId {
    pub fn new(value: impl Into<String>) -> Result<Self, HistoryError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 {
            return Err(HistoryError::InvalidScopeId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
