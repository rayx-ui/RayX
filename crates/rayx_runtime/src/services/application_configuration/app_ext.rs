use super::{
    APPLICATION_CONFIGURATION_PATH, ApplicationConfiguration, ApplicationConfigurationService,
};
use crate::services::platform::{PlatformAppBuilderExt, PlatformInfoService};
use rayx_core::{AppBuilder, ServiceAppBuilderExt as _};

/// Registers the [`ApplicationConfigurationService`] on an app builder.
pub trait ApplicationConfigurationAppBuilderExt: AppBuilder {
    /// Registers the app's [`ApplicationConfigurationService`] unless one is registered, with
    /// the platform service (registered too when missing). Its [`ApplicationConfiguration`]
    /// binds from [`APPLICATION_CONFIGURATION_PATH`], then `configure` changes it; invalid
    /// settings fail startup. When the configuration reloads, the service takes the new
    /// settings.
    fn add_application_configuration(
        self,
        configure: impl Fn(&mut ApplicationConfiguration) + Send + Sync + 'static,
    ) -> Self {
        let mut builder = self
            .add_platform()
            .add_options::<ApplicationConfiguration>(|options| {
                options
                    .bind(APPLICATION_CONFIGURATION_PATH)
                    .configure(configure)
                    .validate(
                        |options| options.validate().is_ok(),
                        "the application configuration is invalid",
                    )
                    .validate_on_start();
            });
        builder
            .services_mut()
            .try_add_service_factory::<ApplicationConfigurationService>(|services| {
                let platform = services
                    .required_service::<PlatformInfoService>()
                    .map(|platform| platform.as_ref().clone())
                    .unwrap_or_else(|error| {
                        panic!("the application configuration needs the platform: {error}")
                    });
                let monitor = services.options_monitor::<ApplicationConfiguration>();
                let options = monitor.current_value().unwrap_or_else(|error| {
                    panic!("the application configuration is invalid: {error}")
                });
                let service = ApplicationConfigurationService::new(options.cloned(), platform)
                    .unwrap_or_else(|error| panic!("{error}"));
                let updated = service.clone();
                // Lives as long as the monitor's options, the root's.
                monitor
                    .on_change(move |options, _name| {
                        // Startup-only and invalid changes keep the settings as they are.
                        let _ = updated.replace_options(options.value().clone());
                    })
                    .detach();
                service
            });
        builder
    }

    /// Registers `service` as the app's [`ApplicationConfigurationService`], replacing any
    /// other.
    fn with_application_configuration_service(
        mut self,
        service: ApplicationConfigurationService,
    ) -> Self {
        self.services_mut().replace_service(service);
        self
    }
}

impl<T> ApplicationConfigurationAppBuilderExt for T where T: AppBuilder {}
