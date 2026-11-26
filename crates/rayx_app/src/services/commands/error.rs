use std::fmt;

use crate::services::ContractFailure;

/// Why a command or context operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandError {
    /// No command with this identifier is registered.
    UnknownCommand(String),
    /// The command is already executing and cannot be re-entered while running.
    AlreadyExecuting(String),
    /// An argument (such as a command identifier or context name) was invalid.
    InvalidArgument(String),
    /// The command's handler returned an error or threw an exception.
    ExecutionFailed { id: String, message: String },
    /// The contract call failed (a TS implementation threw, or the engine is gone).
    Failed(ContractFailure),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(id) => write!(f, "unknown command `{id}`"),
            Self::AlreadyExecuting(id) => write!(f, "command `{id}` is already executing"),
            Self::InvalidArgument(message) => write!(f, "invalid command argument: {message}"),
            Self::ExecutionFailed { id, message } => {
                write!(f, "command `{id}` failed: {message}")
            }
            Self::Failed(failure) => failure.fmt(f),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<ContractFailure> for CommandError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}
