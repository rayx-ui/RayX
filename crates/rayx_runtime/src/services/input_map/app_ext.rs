use super::{INPUT_MAP_OPTIONS_SECTION, InputMapOptions, InputMapService};
use crate::services::storage::{KeyValueStore, StorageAppBuilderExt};
use rayx_core::{AppBuilder, ServiceAppBuilderExt as _};

/// Registers the [`InputMapService`] on an app builder.
pub trait InputMapAppBuilderExt: AppBuilder {
    /// Registers the app's [`InputMapService`] (a startup singleton that reads the user's
    /// bindings from the app's [`KeyValueStore`], registered too when missing) unless one is
    /// registered. Its [`InputMapOptions`] bind from [`INPUT_MAP_OPTIONS_SECTION`], then
    /// `configure` changes them; invalid bindings fail startup.
    fn add_input_map(
        self,
        configure: impl Fn(&mut InputMapOptions) + Send + Sync + 'static,
    ) -> Self {
        let mut builder = self
            .add_storage()
            .add_options::<InputMapOptions>(|options| {
                options
                    .bind(INPUT_MAP_OPTIONS_SECTION)
                    .configure(configure)
                    .validate(
                        |options| {
                            let (framework, application) = options.documents();
                            framework.validate().is_ok() && application.validate().is_ok()
                        },
                        "the input map has an invalid or duplicate binding",
                    )
                    .validate_on_start();
            });
        builder
            .services_mut()
            .try_add_app_service_factory::<InputMapService>(|services| {
                let store = services
                    .required_service::<dyn KeyValueStore>()
                    .unwrap_or_else(|error| {
                        panic!("the input map needs a key-value store: {error}")
                    });
                let options = services
                    .options::<InputMapOptions>()
                    .unwrap_or_else(|error| panic!("the input map's options are invalid: {error}"));
                let (framework, application) = options.documents();
                InputMapService::new(store, framework, application)
                    .unwrap_or_else(|error| panic!("the input map's bindings are invalid: {error}"))
            });
        builder
    }

    /// Registers `service` as the app's [`InputMapService`], replacing any other.
    fn with_input_map_service(mut self, service: InputMapService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> InputMapAppBuilderExt for T where T: AppBuilder {}
