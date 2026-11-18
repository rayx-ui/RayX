use crate::services::ContractFailure;
use crate::services::operations::{OperationApplyStatus, OperationError};

/// Why a history call failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryError {
    /// A scope id was blank or longer than 256 bytes.
    InvalidScopeId,
    /// An entry limit of 0.
    InvalidEntryLimit,
    /// The scope is replaying.
    ScopeBusy,
    /// The scope's stacks changed during the call.
    StackChanged,
    /// The operation service failed.
    Operation(OperationError),
    /// A replayed operation was not applied.
    ApplyStatus(OperationApplyStatus),
    /// The history's store failed (its message).
    Store(String),
    /// The history's state is poisoned (a thread panicked while it held it).
    Poisoned,
    /// A call of the operation service or the store did not answer within the history's
    /// timeout: the command gave up, freeing its scope.
    TimedOut,
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl From<OperationError> for HistoryError {
    fn from(value: OperationError) -> Self {
        Self::Operation(value)
    }
}

impl From<ContractFailure> for HistoryError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
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
            Self::TimedOut => formatter.write_str("history_timed_out"),
            Self::Failed(failure) => write!(formatter, "history_call_failed: {failure}"),
        }
    }
}

impl std::error::Error for HistoryError {}
