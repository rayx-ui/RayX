use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{KeyValueStore, Result, StorageError};

/// A bucket of a [`KeyValueStore`], storing values as JSON: `store.bucket("drafts")`.
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
    pub async fn set_json<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let bytes = serde_json::to_vec(value).map_err(|error| self.serialization(key, error))?;
        self.store
            .set(self.name.clone(), key.to_owned(), bytes)
            .await
    }

    /// The JSON value at `key`; [`StorageError::MissingKey`] when there is none.
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        self.try_get_json(key)
            .await?
            .ok_or_else(|| StorageError::MissingKey {
                bucket: self.name.clone(),
                key: key.to_owned(),
            })
    }

    /// The JSON value at `key`, if there is one.
    pub async fn try_get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.store
            .get(self.name.clone(), key.to_owned())
            .await?
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| self.serialization(key, error))
            })
            .transpose()
    }

    /// Removes the value at `key`; whether there was one.
    pub async fn remove(&self, key: &str) -> Result<bool> {
        self.store.remove(self.name.clone(), key.to_owned()).await
    }

    /// The bucket's keys, sorted.
    pub async fn keys(&self) -> Result<Vec<String>> {
        self.store.keys(self.name.clone()).await
    }

    /// Removes every value in the bucket.
    pub async fn clear(&self) -> Result<()> {
        self.store.clear(self.name.clone()).await
    }

    fn serialization(&self, key: &str, error: serde_json::Error) -> StorageError {
        StorageError::Serialization {
            bucket: self.name.clone(),
            key: key.to_owned(),
            message: error.to_string(),
        }
    }
}
