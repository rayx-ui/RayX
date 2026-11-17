//! The core's services module ([`rayx_app::services`]), re-exported, with the NSH side of RayX's
//! services beside it, a folder each: what NSH implements for a service, its TS class and its TS
//! options. Here too: [`NshServicesAppBuilderExt`], which registers NSH's implementations where
//! the app registered none, and the one helper the TS classes share.
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
pub(crate) mod assets;
pub(crate) mod fonts;
pub(crate) mod i18n;
pub(crate) mod input_map;
pub(crate) mod listeners_ts;
pub(crate) mod navigation;
pub(crate) mod platform;
pub(crate) mod storage;
pub(crate) mod suspension;

use std::sync::Arc;

use nsh_core::JsonValue;
use rayx_app::{AppBuilder, ServiceProvider};
use rayx_runtime::services::assets::AssetReader;
use rayx_runtime::services::fonts::FontRegistrar;
use rayx_runtime::services::navigation::NavigationLocationSynchronizer;
use rayx_runtime::services::storage::{KeyValueStore, PlatformKeyValueStore};
use rayx_runtime::services::suspension::SuspensionHost;
use serde::Serialize;

use crate::services::assets::reader::NshAssetReader;
use crate::services::fonts::registrar::NshFontRegistrar;
use crate::services::navigation::location::NshLocationSynchronizer;
use crate::services::storage::store::NshKeyValueStore;
use crate::services::suspension::host::NshSuspensionHost;

/// `value` as a plain object for TSX.
pub(crate) fn json(value: &impl Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

/// Registers NSH's [`NshAssetReader`] and [`NshFontRegistrar`] as the app's `dyn AssetReader`
/// and `dyn FontRegistrar`, its [`NshKeyValueStore`] (a startup service, which loads on the
/// engine thread) as its `dyn KeyValueStore`, its [`NshLocationSynchronizer`] as its
/// `dyn NavigationLocationSynchronizer` and its [`NshSuspensionHost`] as its
/// `dyn SuspensionHost`, unless others are registered. A TS builder starts with them and the
/// app's start adds them where the app registered none
/// ([`StartAppBuilderExt`](crate::StartAppBuilderExt)), so on NSH `add_storage()` keeps NSH's
/// persistent store (in either order: NSH's store is the platform's
/// [`PlatformKeyValueStore`], which `add_storage()` makes the app's store),
/// `add_navigation()` shows its location as NSH's and `add_suspension()` quits the app.
pub trait NshServicesAppBuilderExt: AppBuilder {
    /// Adds the files a crate ships, as NSH's `assets!()` lists them (the list its UI set
    /// registers), to the asset catalog: `builder.add_asset_files(assets!())`. The crate's
    /// manifest (`assets.toml`) is then optional, for metadata (tags, locale, theme, ...).
    fn add_asset_files(self, files: &[nsh_core::element::AssetFile]) -> Self {
        assets::reader::add_asset_files(self, files)
    }

    /// Registers NSH's services (the trait's list) where the app registered none.
    fn add_nsh_services(mut self) -> Self {
        let services = self.services_mut();
        services.try_add_service_factory_as::<dyn AssetReader>(|_| Arc::new(NshAssetReader));
        services.try_add_service_factory_as::<dyn FontRegistrar>(|_| Arc::new(NshFontRegistrar));
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
                .unwrap_or_else(|error| {
                    panic!("NSH's location synchronizer is registered: {error}")
                })
        });
        services.try_add_service_factory_as::<NshSuspensionHost>(|_| {
            Arc::new(NshSuspensionHost::new())
        });
        services.try_add_service_factory_as::<dyn SuspensionHost>(|services| {
            services
                .required_service::<NshSuspensionHost>()
                .unwrap_or_else(|error| panic!("NSH's suspension host is registered: {error}"))
        });
        self
    }
}

impl<T> NshServicesAppBuilderExt for T where T: AppBuilder {}
