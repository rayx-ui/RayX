use std::sync::Arc;

use super::{HistoryService, HistoryStore};
use crate::services::operations::{OperationService, OperationsAppBuilderExt};
use rayx_core::{AppBuilder, ServiceProvider};

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
}

impl<T> HistoryAppBuilderExt for T where T: AppBuilder {}

fn operations(services: &ServiceProvider) -> OperationService {
    services
        .required_service::<OperationService>()
        .map(|operations| operations.as_ref().clone())
        .unwrap_or_else(|error| panic!("the history service needs the operation service: {error}"))
}
