use super::{ASSET_OPTIONS_SECTION, AssetOptions, AssetReader, AssetService};
use rayx_core::{AppBuilder, ServiceAppBuilderExt as _};

/// Registers the [`AssetService`] on an app builder.
pub trait AssetAppBuilderExt: AppBuilder {
    /// Registers the app's [`AssetService`] (a startup singleton that reads the packages'
    /// manifests) unless one is registered. Its [`AssetOptions`] bind from
    /// [`ASSET_OPTIONS_SECTION`], then `configure` changes them (`options.package("my-app")`).
    /// It reads through the app's `dyn AssetReader` (`rayx_app` registers NSH's).
    fn add_assets(mut self, configure: impl Fn(&mut AssetOptions) + Send + Sync + 'static) -> Self {
        self = self.add_options::<AssetOptions>(|options| {
            options
                .bind(ASSET_OPTIONS_SECTION)
                .configure(configure)
                .validate(
                    |options| !options.manifest.trim().is_empty(),
                    "the asset manifest's file name is empty",
                )
                .validate_on_start();
        });
        self.services_mut()
            .try_add_app_service_factory::<AssetService>(|services| {
                let options = services
                    .options::<AssetOptions>()
                    .unwrap_or_else(|error| panic!("the asset options are invalid: {error}"))
                    .cloned();
                AssetService::new(options, services.service::<dyn AssetReader>())
            });
        self
    }

    /// Registers `service` as the app's [`AssetService`], replacing any other.
    fn with_asset_service(mut self, service: AssetService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> AssetAppBuilderExt for T where T: AppBuilder {}
