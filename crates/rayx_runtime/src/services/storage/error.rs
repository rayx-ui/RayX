/// Why a storage call failed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StorageError {
    /// The store's state is poisoned (a thread panicked while it held it).
    #[error("the storage is poisoned")]
    Poisoned,
    /// [`StorageBucket::get_json`](super::StorageBucket::get_json) found no value.
    #[error("no value at {bucket}/{key}")]
    MissingKey { bucket: String, key: String },
    /// A value did not serialize or deserialize.
    #[error("the value at {bucket}/{key} did not (de)serialize: {message}")]
    Serialization {
        bucket: String,
        key: String,
        message: String,
    },
    /// The store's backend failed.
    #[error("the storage backend failed: {0}")]
    Backend(String),
}

/// The result of a storage call.
pub type Result<T, E = StorageError> = std::result::Result<T, E>;
