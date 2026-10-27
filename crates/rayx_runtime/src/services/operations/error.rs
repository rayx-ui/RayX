use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationError {
    InvalidTargetId,
    DuplicateTarget(String),
    UnknownTarget(String),
    TargetReleased(String),
    UnsupportedOperation { kind: String, version: u32 },
    PayloadTooLarge { size: usize, limit: usize },
    Encode,
    Decode,
    Validation(String),
    Apply(String),
    NestedTransactionRejected,
    TransactionClosed,
    Poisoned,
}

impl OperationError {
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
        }
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for OperationError {}
