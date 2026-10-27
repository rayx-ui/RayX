/// Why an asset call failed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AssetError {
    #[error("`{path}` is not an asset path: {reason}")]
    InvalidPath { path: String, reason: String },
    #[error("the asset manifest of `{package}` is invalid: {reason}")]
    InvalidManifest { package: String, reason: String },
    #[error("no asset `{0}`")]
    NotFound(String),
    #[error("`{uri}` could not be read: {reason}")]
    Read { uri: String, reason: String },
    #[error("`{path}` is not {expected}: {reason}")]
    Decode {
        path: String,
        expected: &'static str,
        reason: String,
    },
    #[error("the asset catalog is not loaded yet; the app has not started")]
    NotLoaded,
}

/// The result of an asset call.
pub type Result<T, E = AssetError> = std::result::Result<T, E>;
