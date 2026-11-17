//! The core's services module ([`rayx_app::services`]), re-exported, with the NSH side of RayX's
//! services beside it, a folder each: what NSH implements for a service, its TS class and its TS
//! options. Here too: the one helper the TS classes share.
//!
//! The services' TS classes ([`I18nService`](crate::ts::I18nService) and the others in
//! [`ts`](crate::ts)): each class carries its Rust service's name and is its own token
//! (`services.getRequiredService(NavigationService)`), and each `onChange` listener hears the
//! service's changes until the function it returned is called (or the reactive owner of the call
//! goes), or the object is disposed. Values cross as plain objects (the services' snapshots,
//! serialized). The services notify on the engine's thread, where TSX listeners run.

#[doc(no_inline)]
pub use rayx_app::services::*;

pub(crate) mod application_configuration;
pub(crate) mod i18n;
pub(crate) mod input_map;
pub(crate) mod listeners_ts;
pub(crate) mod navigation;
pub(crate) mod storage;

use std::sync::Arc;

use nsh_core::JsonValue;
use rayx_app::ServiceProvider;
use rayx_runtime::services::navigation::NavigationLocationSynchronizer;
use rayx_runtime::services::storage::{KeyValueStore, PlatformKeyValueStore};
use serde::Serialize;

use crate::services::navigation::location::NshLocationSynchronizer;
use crate::services::storage::store::NshKeyValueStore;

/// `value` as a plain object for TSX.
pub fn json(value: &impl Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

/// Registers NSH's [`NshKeyValueStore`] (a startup service, which loads on the engine thread)
/// as the app's `dyn KeyValueStore` (and the platform's [`PlatformKeyValueStore`], which
/// `add_storage()` makes the app's store, in either order) and its [`NshLocationSynchronizer`]
/// as its `dyn NavigationLocationSynchronizer`, unless others are registered: the app's start
/// does, until storage and navigation are service crates of their own.
pub(crate) fn add_nsh_storage_and_location(services: &mut ServiceProvider) {
    services.try_add_app_service_factory::<NshKeyValueStore>(|_| NshKeyValueStore::new());
    let nsh_store = |services: &ServiceProvider| -> Arc<dyn KeyValueStore> {
        services
            .required_service::<NshKeyValueStore>()
            .unwrap_or_else(|error| panic!("NSH's key-value store is registered: {error}"))
    };
    services.try_add_service_factory::<PlatformKeyValueStore>(move |services| {
        PlatformKeyValueStore(nsh_store(services))
    });
    services.try_add_service_factory_as::<dyn KeyValueStore>(nsh_store);
    services.try_add_service_factory_as::<NshLocationSynchronizer>(|_| {
        Arc::new(NshLocationSynchronizer::new())
    });
    services.try_add_service_factory_as::<dyn NavigationLocationSynchronizer>(|services| {
        services
            .required_service::<NshLocationSynchronizer>()
            .unwrap_or_else(|error| panic!("NSH's location synchronizer is registered: {error}"))
    });
}
