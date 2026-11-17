//! The store in memory.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_app::services::storage::{KeyValueStore, Result, StorageCapabilities, StorageError};

type Buckets = HashMap<String, BTreeMap<String, Vec<u8>>>;

/// A [`KeyValueStore`] in memory: values live as long as the store (clones share them), and
/// every call answers at once. `add_in_memory_storage()` registers one.
#[derive(Clone, Debug, Default)]
pub struct InMemoryKeyValueStore {
    buckets: Arc<Mutex<Buckets>>,
}

impl InMemoryKeyValueStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Buckets) -> R,
    ) -> BoxFuture<'_, Result<R>> {
        let answer = self
            .buckets
            .lock()
            .map(|mut buckets| f(&mut buckets))
            .map_err(|_| StorageError::Poisoned);
        futures::future::ready(answer).boxed()
    }
}

impl KeyValueStore for InMemoryKeyValueStore {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::default()
    }

    fn get(&self, bucket: String, key: String) -> BoxFuture<'_, Result<Option<Vec<u8>>>> {
        self.with(move |buckets| {
            buckets
                .get(&bucket)
                .and_then(|values| values.get(&key))
                .cloned()
        })
    }

    fn set(&self, bucket: String, key: String, value: Vec<u8>) -> BoxFuture<'_, Result<()>> {
        self.with(move |buckets| {
            buckets.entry(bucket).or_default().insert(key, value);
        })
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool>> {
        self.with(move |buckets| {
            buckets
                .get_mut(&bucket)
                .is_some_and(|values| values.remove(&key).is_some())
        })
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>>> {
        self.with(move |buckets| {
            buckets
                .get(&bucket)
                .map(|values| values.keys().cloned().collect())
                .unwrap_or_default()
        })
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<()>> {
        self.with(move |buckets| {
            buckets.remove(&bucket);
        })
    }
}
