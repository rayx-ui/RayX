use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use super::{KeyValueStore, Result, StorageCapabilities, StorageError};

type Buckets = HashMap<String, BTreeMap<String, Vec<u8>>>;

/// A [`KeyValueStore`] in memory: values live as long as the store (clones share them).
#[derive(Clone, Debug, Default)]
pub struct InMemoryKeyValueStore {
    buckets: Arc<Mutex<Buckets>>,
}

impl InMemoryKeyValueStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut Buckets) -> R) -> Result<R> {
        let mut buckets = self.buckets.lock().map_err(|_| StorageError::Poisoned)?;
        Ok(f(&mut buckets))
    }
}

impl KeyValueStore for InMemoryKeyValueStore {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::default()
    }

    fn get(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
        self.with(|buckets| {
            buckets
                .get(bucket)
                .and_then(|values| values.get(key))
                .cloned()
        })
    }

    fn set(&self, bucket: &str, key: &str, value: Vec<u8>) -> Result<()> {
        self.with(|buckets| {
            buckets
                .entry(bucket.to_owned())
                .or_default()
                .insert(key.to_owned(), value);
        })
    }

    fn remove(&self, bucket: &str, key: &str) -> Result<bool> {
        self.with(|buckets| {
            buckets
                .get_mut(bucket)
                .is_some_and(|values| values.remove(key).is_some())
        })
    }

    fn keys(&self, bucket: &str) -> Result<Vec<String>> {
        self.with(|buckets| {
            buckets
                .get(bucket)
                .map(|values| values.keys().cloned().collect())
                .unwrap_or_default()
        })
    }

    fn clear(&self, bucket: &str) -> Result<()> {
        self.with(|buckets| {
            buckets.remove(bucket);
        })
    }
}
