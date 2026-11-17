//! Key-value storage: values as bytes under a key in a named bucket, kept by a
//! [`KeyValueStore`] implementation, with JSON helpers ([`StorageBucket`], from
//! `store.bucket(name)` on `dyn KeyValueStore`).
//!
//! RayX's implementations are `rayx_services_storage`'s: `add_storage()` keeps the values in NSH's
//! storage (the device's, persistent), `add_in_memory_storage()` for the app's life. An app may
//! register its own `dyn KeyValueStore`, in Rust or in TS (a keychain, a database).

mod bucket;
mod error;

use serde::{Deserialize, Serialize};

pub use bucket::StorageBucket;
pub use error::{Result, StorageError};

/// What a store keeps its values in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageCapabilities {
    /// Values outlive the process.
    pub persistent: bool,
    /// Values are encrypted at rest (a keychain, a keystore).
    pub secure: bool,
}

/// A key-value store: values are bytes, under a key in a named bucket, read and written from any
/// thread.
#[rayx_app_macros::contract(ts = "KeyValueStoreContract")]
pub trait KeyValueStore {
    /// What the store keeps its values in.
    #[constant(ts = "StorageCapabilities")]
    fn capabilities(&self) -> StorageCapabilities;

    /// The value at `key` in `bucket`, if there is one.
    #[ts("(bucket: string, key: string): number[] | null")]
    async fn get(&self, bucket: String, key: String) -> Result<Option<Vec<u8>>, StorageError>;

    /// Sets the value at `key` in `bucket`.
    #[ts("(bucket: string, key: string, value: number[])")]
    async fn set(&self, bucket: String, key: String, value: Vec<u8>) -> Result<(), StorageError>;

    /// Removes the value at `key` in `bucket`; whether there was one.
    async fn remove(&self, bucket: String, key: String) -> Result<bool, StorageError>;

    /// The keys in `bucket`, sorted.
    async fn keys(&self, bucket: String) -> Result<Vec<String>, StorageError>;

    /// Removes every value in `bucket`.
    async fn clear(&self, bucket: String) -> Result<(), StorageError>;
}

impl dyn KeyValueStore {
    /// The bucket `name`, for JSON values.
    pub fn bucket(&self, name: impl Into<String>) -> StorageBucket<'_> {
        StorageBucket::new(self, name.into())
    }
}
