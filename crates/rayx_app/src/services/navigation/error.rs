use crate::services::ContractFailure;

/// Why a navigation failed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NavigationError {
    #[error("duplicate route: {0}")]
    DuplicateRoute(String),
    #[error("invalid route pattern `{pattern}`: {message}")]
    InvalidRoutePattern { pattern: String, message: String },
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("navigation state lock was poisoned")]
    Poisoned,
    /// A location the service cannot follow (outside the base path, a journal entry it does
    /// not have).
    #[error("{0}")]
    Location(String),
    /// The call failed (a TS implementation threw, the engine is gone).
    #[error("{0}")]
    Failed(ContractFailure),
}

impl From<ContractFailure> for NavigationError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The result of a navigation call.
pub type Result<T, E = NavigationError> = std::result::Result<T, E>;
