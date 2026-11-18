//! `add_history`, `add_history_with_store` and `add_persistent_history`: in Rust
//! ([`HistoryAppBuilderExt`]) and in TS ([`HistoryBuilder`], a class extension of the host's
//! `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::history::HistoryService;
use rayx_app::services::operations::OperationService;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::{AppBuilder, ServiceProvider};
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::key_value_store::KeyValueHistoryStore;
use crate::service::{RayxHistory, Starting};
use crate::store::HistoryStore;
use crate::timer::{HISTORY_CONFIG_PATH, HistoryOptions, HistoryTimer};
use rayx_app_nsh::EngineRunner;

/// What an app with a history and no operation service is told.
const OPERATIONS_HINT: &str = "add_operations() (addOperations in TS) from \
                               rayx_services_operations, or register your own dyn \
                               OperationService";

/// What an app keeping its history in its key-value store without one is told.
const STORAGE_HINT: &str = "add_storage() (addStorage in TS) from rayx_services_storage, or \
                            register your own dyn KeyValueStore";

/// Registers RayX's history on an app builder.
pub trait HistoryAppBuilderExt: AppBuilder {
    /// Registers RayX's history as the app's `dyn HistoryService`, replacing any other: a
    /// startup service observing the app's `dyn OperationService` (which it requires:
    /// `add_operations()` from `rayx_services_operations`, or the app's own), with history kept
    /// in memory only.
    fn add_history(self) -> Self {
        register(self, false)
    }

    /// Like [`add_history`](Self::add_history), with each scope's history persisted in
    /// `store`, which is registered as the app's `dyn HistoryStore` (a startup service the
    /// history waits for).
    fn add_history_with_store(mut self, store: impl HistoryStore) -> Self {
        let store: Arc<dyn HistoryStore> = Arc::new(store);
        self.services_mut()
            .replace_app_service::<dyn HistoryStore>(store);
        register(self, true)
    }

    /// Like [`add_history`](Self::add_history), with each scope's history kept in the app's
    /// `dyn KeyValueStore` (which it requires: `add_storage()` from `rayx_services_storage`, or
    /// the app's own) by a [`KeyValueHistoryStore`], so it persists wherever the store does: on
    /// NSH, in the device's storage.
    fn add_persistent_history(mut self) -> Self {
        let services = self.services_mut();
        services.require_for::<dyn HistoryStore, dyn KeyValueStore>(STORAGE_HINT);
        services.replace_app_service_factory_as::<dyn HistoryStore>(|services| {
            let store = services
                .required_service::<dyn KeyValueStore>()
                .unwrap_or_else(|error| {
                    panic!("the history store needs a key-value store: {error}")
                });
            Arc::new(KeyValueHistoryStore::new(store))
        });
        register(self, true)
    }
}

impl<T> HistoryAppBuilderExt for T where T: AppBuilder {}

/// Registers RayX's history as `dyn HistoryService` (a startup service keyed by it), kept in
/// the app's `dyn HistoryStore` when `stored`.
fn register<B: AppBuilder>(mut builder: B, stored: bool) -> B {
    let services = builder.services_mut();
    services.require_for::<dyn HistoryService, dyn OperationService>(OPERATIONS_HINT);
    services
        .add_options::<HistoryOptions>()
        .bind(HISTORY_CONFIG_PATH);
    services.replace_service_factory_as::<RayxHistory>(move |services| {
        let timeout = services
            .options::<HistoryOptions>()
            .unwrap_or_else(|error| panic!("the history options are invalid: {error}"))
            .call_timeout();
        // The app's timer, else the engine's on NSH; with neither, calls are unbounded.
        let timer = services
            .service::<HistoryTimer>()
            .map(|timer| (*timer).clone())
            .or_else(|| {
                services
                    .service::<EngineRunner>()
                    .map(|runner| HistoryTimer::engine((*runner).clone()))
            });
        RayxHistory::new(operations(services), None, stored, timer, timeout)
    });
    // Replaced before the startup service is added: replacing `dyn HistoryService` drops the
    // startup service keyed by it, which `Starting` is.
    services.replace_service_factory_as::<dyn HistoryService>(|services| {
        services
            .required_service::<RayxHistory>()
            .unwrap_or_else(|error| panic!("RayX's history is registered: {error}"))
    });
    services.add_startup_resolved_async(
        StartupServiceType::of::<dyn HistoryService>(),
        |context| {
            let history = context.services().required_service::<RayxHistory>();
            Box::pin(async move {
                let history = history.map_err(|error| StartupInitializationFailure {
                    service: StartupServiceType::of::<dyn HistoryService>(),
                    message: error.to_string(),
                    timed_out: false,
                })?;
                Ok(Arc::new(Starting(history)) as StartupService)
            })
        },
    );
    builder
}

fn operations(services: &ServiceProvider) -> Arc<dyn OperationService> {
    services
        .required_service::<dyn OperationService>()
        .unwrap_or_else(|error| panic!("the history needs an operation service: {error}"))
}

/// The methods `rayx_services_history` adds to TS's `RayXAppBuilder`.
pub struct HistoryBuilder;

#[class_extension(RayXAppBuilder)]
impl HistoryBuilder {
    /// Registers the app's history (undo and redo of operations, kept in memory); it needs an
    /// operation service (`addOperations`).
    pub fn add_history(this: Instance<RayXAppBuilder>) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, HistoryAppBuilderExt::add_history)
    }

    /// Registers the app's history kept in its key-value store (so on NSH in the device's
    /// storage); it needs an operation service (`addOperations`) and a key-value store
    /// (`addStorage`).
    pub fn add_persistent_history(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, HistoryAppBuilderExt::add_persistent_history)
    }
}
