#[derive(Debug, thiserror::Error)]
pub enum NavigationError {
    #[error("duplicate route: {0}")]
    DuplicateRoute(String),
    #[error("invalid route pattern `{pattern}`: {message}")]
    InvalidRoutePattern { pattern: String, message: String },
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("navigation state lock was poisoned")]
    Poisoned,
}

pub type Result<T> = std::result::Result<T, NavigationError>;
