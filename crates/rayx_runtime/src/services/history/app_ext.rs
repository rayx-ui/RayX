use std::sync::Arc;

use super::{HistoryService, HistoryStore, KeyValueHistoryStore};
use crate::services::operations::{OperationService, OperationsAppBuilderExt};
use crate::services::storage::KeyValueStore;
use rayx_app::{AppBuilder, ServiceProvider};

/// What an app keeping its history in its key-value store without one is told.
const STORAGE_HINT: &str = "add_storage() (addStorage in TS) from rayx_services_storage, or \
                            register your own dyn KeyValueStore";

/// Registers the [`HistoryService`] (undo and redo of operations) on an app builder.
pub trait HistoryAppBuilderExt: AppBuilder {
    /// Registers the app's [`HistoryService`], a startup singleton observing the app's
    /// [`OperationService`] (registered too when missing), with history kept in memory only.
    fn add_history(self) -> Self {
        let mut builder = self.add_operations();
        builder
            .services_mut()
            .try_add_app_service_factory::<HistoryService>(|services| {
                HistoryService::new(operations(services))
                    .unwrap_or_else(|error| panic!("the history service did not start: {error}"))
            });
        builder
    }

    /// Like [`add_history`](Self::add_history), with each scope's history persisted in
    /// `store`, which is registered as the app's `dyn HistoryStore` (a startup service the
    /// history waits for).
    fn add_history_with_store(self, store: impl HistoryStore) -> Self {
        let mut builder = self.add_operations();
        let store: Arc<dyn HistoryStore> = Arc::new(store);
        builder
            .services_mut()
            .replace_app_service::<dyn HistoryStore>(store);
        builder
            .services_mut()
            .replace_app_service_factory::<HistoryService>(|services| {
                HistoryService::with_configured_store(operations(services))
                    .unwrap_or_else(|error| panic!("the history service did not start: {error}"))
            });
        builder
    }

    /// Like [`add_history`](Self::add_history), with each scope's history kept in the app's
    /// `dyn KeyValueStore` (which it requires: `add_storage()` from `rayx_services_storage`, or
    /// the app's own) by a [`KeyValueHistoryStore`], so it persists wherever the store does: on
    /// NSH, in the device's storage.
    fn add_persistent_history(self) -> Self {
        let mut builder = self.add_operations();
        builder
            .services_mut()
            .require_for::<dyn HistoryStore, dyn KeyValueStore>(STORAGE_HINT);
        builder
            .services_mut()
            .replace_app_service_factory_as::<dyn HistoryStore>(|services| {
                let store = services
                    .required_service::<dyn KeyValueStore>()
                    .unwrap_or_else(|error| {
                        panic!("the history store needs a key-value store: {error}")
                    });
                Arc::new(KeyValueHistoryStore::new(store))
            });
        builder
            .services_mut()
            .replace_app_service_factory::<HistoryService>(|services| {
                HistoryService::with_configured_store(operations(services))
                    .unwrap_or_else(|error| panic!("the history service did not start: {error}"))
            });
        builder
    }
}

impl<T> HistoryAppBuilderExt for T where T: AppBuilder {}

fn operations(services: &ServiceProvider) -> OperationService {
    services
        .required_service::<OperationService>()
        .map(|operations| operations.as_ref().clone())
        .unwrap_or_else(|error| panic!("the history service needs the operation service: {error}"))
}
