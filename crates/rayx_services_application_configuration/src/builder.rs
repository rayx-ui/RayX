//! `add_application_configuration`: in Rust ([`ApplicationConfigurationAppBuilderExt`]) and in
//! TS ([`ApplicationConfigurationBuilder`], a class extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::services::application_configuration::{
    ApplicationConfiguration, ApplicationConfigurationService,
};
use rayx_app::services::platform::PlatformInfoService;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _};
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::APPLICATION_CONFIGURATION_PATH;
use crate::options_ts::ApplicationConfigurationOptions;
use crate::service::RayxConfiguration;

/// What an app registering RayX's application configuration without a platform service is told.
const PLATFORM_HINT: &str = "add_platform() (addPlatform in TS) from rayx_services_platform, or \
                             register your own dyn PlatformInfoService";

/// Registers RayX's application configuration on an app builder.
pub trait ApplicationConfigurationAppBuilderExt: AppBuilder {
    /// Registers RayX's application configuration as the app's
    /// `dyn ApplicationConfigurationService`, replacing any other; it follows the user's motion
    /// setting from the app's `dyn PlatformInfoService`, which it requires (`add_platform()`
    /// from `rayx_services_platform`, or the app's own). Its [`ApplicationConfiguration`] binds
    /// from [`APPLICATION_CONFIGURATION_PATH`], then `configure` changes it; invalid settings
    /// fail startup. When the configuration reloads, the service takes the new settings.
    fn add_application_configuration(
        self,
        configure: impl Fn(&mut ApplicationConfiguration) + Send + Sync + 'static,
    ) -> Self {
        let mut builder = self.add_options::<ApplicationConfiguration>(|options| {
            options
                .bind(APPLICATION_CONFIGURATION_PATH)
                .configure(configure)
                .validate(
                    |options| options.validate().is_ok(),
                    "the application configuration is invalid",
                )
                .validate_on_start();
        });
        let services = builder.services_mut();
        services.require_for::<dyn ApplicationConfigurationService, dyn PlatformInfoService>(
            PLATFORM_HINT,
        );
        services.replace_service_factory_as::<RayxConfiguration>(|services| {
            let platform = services
                .required_service::<dyn PlatformInfoService>()
                .unwrap_or_else(|error| {
                    panic!("the application configuration needs the platform: {error}")
                });
            let monitor = services.options_monitor::<ApplicationConfiguration>();
            let options = monitor.current_value().unwrap_or_else(|error| {
                panic!("the application configuration is invalid: {error}")
            });
            let service = RayxConfiguration::new(options.cloned(), &platform)
                .unwrap_or_else(|error| panic!("{error}"));
            let updated = Arc::downgrade(&service);
            // Lives as long as the monitor's options, the root's.
            monitor
                .on_change(move |options, _name| {
                    // Startup-only and invalid changes keep the settings as they are.
                    if let Some(service) = updated.upgrade() {
                        let _ = service.replace(options.value().clone());
                    }
                })
                .detach();
            service
        });
        services.replace_service_factory_as::<dyn ApplicationConfigurationService>(|services| {
            services
                .required_service::<RayxConfiguration>()
                .unwrap_or_else(|error| {
                    panic!("RayX's application configuration is registered: {error}")
                })
        });
        builder
    }
}

impl<T> ApplicationConfigurationAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_application_configuration` adds to TS's `RayXAppBuilder`.
pub struct ApplicationConfigurationBuilder;

#[class_extension(RayXAppBuilder)]
impl ApplicationConfigurationBuilder {
    /// Registers the app's configuration service (`RayX:Application`: its layout direction,
    /// motion and other settings; it needs a platform service, `addPlatform`); `configure` sets
    /// its settings (`options.name("My App")`) after configuration's.
    pub fn add_application_configuration(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<ApplicationConfigurationOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let steps = record(
            ApplicationConfigurationOptions::new(),
            configure,
            ApplicationConfigurationOptions::take,
        )?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_application_configuration(move |settings| {
                for step in &steps {
                    step.apply(settings);
                }
            })
        })
    }
}
