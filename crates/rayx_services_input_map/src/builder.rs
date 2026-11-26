//! `add_input_map`: in Rust ([`InputMapAppBuilderExt`]) and in TS ([`InputMapBuilder`], a class
//! extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::assets::AssetService;
use rayx_app::services::input_map::InputMapService;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _};
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::options::{INPUT_MAP_OPTIONS_SECTION, InputMapOptions};
use crate::options_ts::InputMapOptions as TsInputMapOptions;
use crate::service::{Loading, RayxInputMap};

/// What an app registering RayX's input map without a key-value store is told.
const STORAGE_HINT: &str = "add_storage() (addStorage in TS) from rayx_services_storage, or \
                            register your own dyn KeyValueStore";

/// Registers RayX's input map on an app builder.
pub trait InputMapAppBuilderExt: AppBuilder {
    /// Registers RayX's input map as the app's `dyn InputMapService`, replacing any other: a
    /// startup service that reads any configured application asset (from `dyn AssetService`,
    /// after it initializes) and the user's bindings from the app's `dyn KeyValueStore` once
    /// it is ready (which it requires: `add_storage()` from `rayx_services_storage`, or the
    /// app's own). Its [`InputMapOptions`] bind from [`INPUT_MAP_OPTIONS_SECTION`], then
    /// `configure` changes them; invalid bindings fail startup.
    fn add_input_map(
        self,
        configure: impl Fn(&mut InputMapOptions) + Send + Sync + 'static,
    ) -> Self {
        let mut builder = self.add_options::<InputMapOptions>(|options| {
            options
                .bind(INPUT_MAP_OPTIONS_SECTION)
                .configure(configure)
                .validate(
                    InputMapOptions::is_valid,
                    "the input map has an invalid or duplicate binding",
                )
                .validate_on_start();
        });
        let services = builder.services_mut();
        services.require_for::<dyn InputMapService, dyn KeyValueStore>(STORAGE_HINT);
        services.replace_service_factory_as::<RayxInputMap>(|services| {
            let store = services
                .required_service::<dyn KeyValueStore>()
                .unwrap_or_else(|error| panic!("the input map needs a key-value store: {error}"));
            let assets = services.service::<dyn AssetService>();
            let options = services
                .options::<InputMapOptions>()
                .unwrap_or_else(|error| panic!("the input map's options are invalid: {error}"));
            let (framework, application) = options.documents();
            RayxInputMap::new(
                store,
                assets,
                framework,
                application,
                options.application_asset.clone(),
            )
            .unwrap_or_else(|error| panic!("the input map's bindings are invalid: {error}"))
        });
        // Replacing drops the startup service keyed `dyn InputMapService` (an earlier one's), so
        // the service's own startup registration, under that key, comes after.
        services.replace_service_factory_as::<dyn InputMapService>(|services| {
            services
                .required_service::<RayxInputMap>()
                .unwrap_or_else(|error| panic!("RayX's input map is registered: {error}"))
        });
        services.add_startup_resolved_async(
            StartupServiceType::of::<dyn InputMapService>(),
            |context| {
                let input_map = context.services().required_service::<RayxInputMap>();
                Box::pin(async move {
                    let input_map = input_map.map_err(|error| StartupInitializationFailure {
                        service: StartupServiceType::of::<dyn InputMapService>(),
                        message: error.to_string(),
                        timed_out: false,
                    })?;
                    Ok(Arc::new(Loading(input_map)) as StartupService)
                })
            },
        );
        builder
    }
}

impl<T> InputMapAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_input_map` adds to TS's `RayXAppBuilder`.
pub struct InputMapBuilder;

#[class_extension(RayXAppBuilder)]
impl InputMapBuilder {
    /// Registers the app's input map (it keeps the user's bindings in the app's key-value store:
    /// `addStorage`); `configure` adds the app's bindings (`options.bindKey("Editor", "ctrl-s", "save")`)
    /// or default asset (`options.applicationAsset("keybindings.json")`) after configuration's options.
    pub fn add_input_map(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsInputMapOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(TsInputMapOptions::new(), configure, TsInputMapOptions::take)?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_input_map(move |options| {
                for step in &steps {
                    step.apply(options);
                }
            })
        })
    }
}
