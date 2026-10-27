use super::{NavigationOptions, NavigationService};
use rayx_core::{AppBuilder, ServiceAppBuilderExt as _};

/// The configuration section [`NavigationOptions`] bind from.
pub const NAVIGATION_OPTIONS_SECTION: &str = "RayX:Services:Navigation";

/// Registers the [`NavigationService`] on an app builder.
pub trait NavigationAppBuilderExt: AppBuilder {
    /// Registers the app's [`NavigationService`] (a startup singleton) unless one is registered,
    /// made from [`NavigationOptions`]: the configuration's [`NAVIGATION_OPTIONS_SECTION`] binds
    /// them, then `configure` changes them in code (.NET's `Bind(..).Configure(..)`). Invalid
    /// routes fail startup.
    ///
    /// Call it once; more steps go through `configure_options::<NavigationOptions>(..)`.
    fn add_navigation(
        mut self,
        configure: impl Fn(&mut NavigationOptions) + Send + Sync + 'static,
    ) -> Self {
        self = self.add_options::<NavigationOptions>(|options| {
            options
                .bind(NAVIGATION_OPTIONS_SECTION)
                .configure(configure)
                .validate(
                    |options| NavigationService::with_options(options.clone()).is_ok(),
                    "the navigation's routes are invalid",
                )
                .validate_on_start();
        });
        self.services_mut()
            .try_add_app_service_factory::<NavigationService>(|services| {
                let options = services
                    .options::<NavigationOptions>()
                    .unwrap_or_else(|error| panic!("the navigation's options are invalid: {error}"))
                    .cloned();
                NavigationService::with_options(options)
                    .unwrap_or_else(|error| panic!("the navigation's routes are invalid: {error}"))
            });
        self
    }

    /// Registers `service` as the app's [`NavigationService`], replacing any other.
    fn with_navigation_service(mut self, service: NavigationService) -> Self {
        self.services_mut().replace_app_service(service);
        self
    }
}

impl<T> NavigationAppBuilderExt for T where T: AppBuilder {}
