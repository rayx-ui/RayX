use super::{HistoryScopeId, HistoryStoreError};
use crate::services::operations::{OperationApplyStatus, OperationError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryError {
    InvalidScopeId,
    InvalidEntryLimit,
    ScopeBusy,
    StackChanged,
    Operation(OperationError),
    ApplyStatus(OperationApplyStatus),
    Store(HistoryStoreError),
    Poisoned,
}

impl From<OperationError> for HistoryError {
    fn from(value: OperationError) -> Self {
        Self::Operation(value)
    }
}

impl From<HistoryStoreError> for HistoryError {
    fn from(value: HistoryStoreError) -> Self {
        Self::Store(value)
    }
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEntryLimit => formatter.write_str("invalid_history_entry_limit"),
            Self::ScopeBusy => formatter.write_str("history_scope_busy"),
            Self::InvalidScopeId => formatter.write_str("invalid_history_scope"),
            Self::StackChanged => formatter.write_str("history_stack_changed"),
            Self::Operation(error) => write!(formatter, "operation_{}", error.code()),
            Self::ApplyStatus(status) => write!(formatter, "history_apply_status_{status:?}"),
            Self::Store(error) => write!(formatter, "history_store_{error}"),
            Self::Poisoned => formatter.write_str("history_state_poisoned"),
        }
    }
}

impl std::error::Error for HistoryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryNavigationFailure {
    pub scope_id: HistoryScopeId,
    pub error_code: String,
}
