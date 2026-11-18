//! `add_storage`: in Rust ([`StorageAppBuilderExt`]) and in TS ([`StorageBuilder`], a class
//! extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::storage::KeyValueStore;
use rayx_app_nsh::EngineRunner;
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::memory::InMemoryKeyValueStore;
use crate::nsh_store::NshKeyValueStore;

/// Registers RayX's key-value store on an app builder.
pub trait StorageAppBuilderExt: AppBuilder {
    /// Registers RayX's store on NSH's storage as the app's `dyn KeyValueStore`, replacing any
    /// other: a startup service that loads what the device's storage holds on the engine thread
    /// when the app starts, then reads and writes from any thread, and writes what waits as the
    /// app quits (heard through the app's `dyn SuspensionService`). Its values persist.
    fn add_storage(mut self) -> Self {
        let services = self.services_mut();
        services.replace_service_factory_as::<NshKeyValueStore>(|services| {
            Arc::new(NshKeyValueStore::new(
                services
                    .service::<EngineRunner>()
                    .map(|runner| (*runner).clone()),
            ))
        });
        // Replaced before the startup service is added: replacing `dyn KeyValueStore` drops the
        // startup services keyed by it.
        services.replace_service_factory_as::<dyn KeyValueStore>(|services| {
            services
                .required_service::<NshKeyValueStore>()
                .unwrap_or_else(|error| panic!("RayX's key-value store is registered: {error}"))
        });
        services.add_startup_resolved_async(
            StartupServiceType::of::<dyn KeyValueStore>(),
            |context| {
                let store = context.services().required_service::<NshKeyValueStore>();
                Box::pin(async move {
                    let store = store.map_err(|error| StartupInitializationFailure {
                        service: StartupServiceType::of::<dyn KeyValueStore>(),
                        message: error.to_string(),
                        timed_out: false,
                    })?;
                    Ok(store as StartupService)
                })
            },
        );
        self
    }

    /// Registers an [`InMemoryKeyValueStore`] as the app's `dyn KeyValueStore`, replacing any
    /// other: values kept for the app's life.
    fn add_in_memory_storage(mut self) -> Self {
        self.services_mut()
            .replace_service_factory_as::<dyn KeyValueStore>(|_| {
                Arc::new(InMemoryKeyValueStore::new())
            });
        self
    }
}

impl<T> StorageAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_storage` adds to TS's `RayXAppBuilder`.
pub struct StorageBuilder;

#[class_extension(RayXAppBuilder)]
impl StorageBuilder {
    /// Registers the app's key-value store (on NSH, NSH's persistent storage), replacing any
    /// other.
    pub fn add_storage(this: Instance<RayXAppBuilder>) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, StorageAppBuilderExt::add_storage)
    }

    /// Registers a key-value store kept in memory for the app's life as the app's key-value
    /// store, replacing any other: nothing persists.
    pub fn add_in_memory_storage(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, StorageAppBuilderExt::add_in_memory_storage)
    }
}
