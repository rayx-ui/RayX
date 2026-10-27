use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{KeyValueStore, Result, StorageError};

/// A bucket of a [`KeyValueStore`], storing values as JSON.
pub struct StorageBucket<'a> {
    store: &'a dyn KeyValueStore,
    name: String,
}

impl<'a> StorageBucket<'a> {
    pub(super) fn new(store: &'a dyn KeyValueStore, name: String) -> Self {
        Self { store, name }
    }

    /// The bucket's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Stores `value` at `key` as JSON.
    pub fn set_json<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let bytes = serde_json::to_vec(value).map_err(|error| self.serialization(key, error))?;
        self.store.set(&self.name, key, bytes)
    }

    /// The JSON value at `key`; [`StorageError::MissingKey`] when there is none.
    pub fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        self.try_get_json(key)?
            .ok_or_else(|| StorageError::MissingKey {
                bucket: self.name.clone(),
                key: key.to_owned(),
            })
    }

    /// The JSON value at `key`, if there is one.
    pub fn try_get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.store
            .get(&self.name, key)?
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| self.serialization(key, error))
            })
            .transpose()
    }

    /// [`set_json`](Self::set_json), through the store's async method.
    pub async fn set_json_async<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let bytes = serde_json::to_vec(value).map_err(|error| self.serialization(key, error))?;
        self.store.set_async(&self.name, key, bytes).await
    }

    /// [`try_get_json`](Self::try_get_json), through the store's async method.
    pub async fn try_get_json_async<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.store
            .get_async(&self.name, key)
            .await?
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| self.serialization(key, error))
            })
            .transpose()
    }

    /// Removes the value at `key`; whether there was one.
    pub fn remove(&self, key: &str) -> Result<bool> {
        self.store.remove(&self.name, key)
    }

    /// The bucket's keys, sorted.
    pub fn keys(&self) -> Result<Vec<String>> {
        self.store.keys(&self.name)
    }

    /// Removes every value in the bucket.
    pub fn clear(&self) -> Result<()> {
        self.store.clear(&self.name)
    }

    fn serialization(&self, key: &str, error: serde_json::Error) -> StorageError {
        StorageError::Serialization {
            bucket: self.name.clone(),
            key: key.to_owned(),
            message: error.to_string(),
        }
    }
}
