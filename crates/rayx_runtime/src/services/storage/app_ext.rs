use std::sync::Arc;

use super::{InMemoryKeyValueStore, KeyValueStore};
use rayx_core::AppBuilder;

/// Registers the app's [`KeyValueStore`] on an app builder.
pub trait StorageAppBuilderExt: AppBuilder {
    /// Registers an [`InMemoryKeyValueStore`] as the app's `dyn KeyValueStore`, unless a store is
    /// registered.
    fn add_storage(mut self) -> Self {
        self.services_mut()
            .try_add_service_factory_as::<dyn KeyValueStore>(|_| {
                Arc::new(InMemoryKeyValueStore::new())
            });
        self
    }

    /// Registers `store` as the app's `dyn KeyValueStore`, replacing any other.
    fn with_key_value_store(mut self, store: impl KeyValueStore) -> Self {
        let store: Arc<dyn KeyValueStore> = Arc::new(store);
        self.services_mut()
            .replace_service::<dyn KeyValueStore>(store);
        self
    }
}

impl<T> StorageAppBuilderExt for T where T: AppBuilder {}
