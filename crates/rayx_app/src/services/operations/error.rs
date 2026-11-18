use std::fmt;

use crate::services::ContractFailure;

/// Why an operation call failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationError {
    /// A target id was blank or longer than 256 bytes.
    InvalidTargetId,
    /// A target of this id is registered already.
    DuplicateTarget(String),
    /// No target of this id was ever registered.
    UnknownTarget(String),
    /// The target of this id was released.
    TargetReleased(String),
    /// A record of another kind or version than its target's.
    UnsupportedOperation { kind: String, version: u32 },
    /// An operation larger than its limit.
    PayloadTooLarge { size: usize, limit: usize },
    /// An operation did not encode.
    Encode,
    /// A record did not decode.
    Decode,
    /// The target refused the operation (its message).
    Validation(String),
    /// The target failed to apply the operation (its message).
    Apply(String),
    /// A transaction began in `Reject` mode while one is open on the thread.
    NestedTransactionRejected,
    /// The transaction is closed (committed or aborted) or unknown.
    TransactionClosed,
    /// The service's state is poisoned (a thread panicked while it held it).
    Poisoned,
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl OperationError {
    /// The error's code, for diagnostics and failure events.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidTargetId => "invalid_target_id",
            Self::DuplicateTarget(_) => "duplicate_target",
            Self::UnknownTarget(_) => "unknown_target",
            Self::TargetReleased(_) => "target_released",
            Self::UnsupportedOperation { .. } => "unsupported_operation",
            Self::PayloadTooLarge { .. } => "payload_too_large",
            Self::Encode => "encode_failed",
            Self::Decode => "decode_failed",
            Self::Validation(_) => "validation_failed",
            Self::Apply(_) => "apply_failed",
            Self::NestedTransactionRejected => "nested_transaction_rejected",
            Self::TransactionClosed => "transaction_closed",
            Self::Poisoned => "operation_state_poisoned",
            Self::Failed(_) => "operation_call_failed",
        }
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(failure) => write!(f, "{}: {failure}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for OperationError {}

impl From<ContractFailure> for OperationError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}
