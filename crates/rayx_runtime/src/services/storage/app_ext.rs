use std::sync::Arc;

use super::{InMemoryKeyValueStore, KeyValueStore};
use rayx_app::AppBuilder;

/// The store the platform keeps the app's values in (on NSH, NSH's storage), registered by the
/// UI layer: [`StorageAppBuilderExt::add_storage`] makes it the app's store, whether the layer
/// registered it before or after.
pub struct PlatformKeyValueStore(pub Arc<dyn KeyValueStore>);

/// Registers the app's [`KeyValueStore`] on an app builder.
pub trait StorageAppBuilderExt: AppBuilder {
    /// Registers the platform's store ([`PlatformKeyValueStore`]) as the app's
    /// `dyn KeyValueStore`, or an [`InMemoryKeyValueStore`] when the platform has none, unless a
    /// store is registered.
    fn add_storage(mut self) -> Self {
        self.services_mut()
            .try_add_service_factory_as::<dyn KeyValueStore>(|services| match services
                .service::<PlatformKeyValueStore>(
            ) {
                Some(platform) => Arc::clone(&platform.0),
                None => Arc::new(InMemoryKeyValueStore::new()),
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
