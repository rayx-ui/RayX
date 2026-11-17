//! `add_navigation`: in Rust ([`NavigationAppBuilderExt`]) and in TS ([`NavigationBuilder`], a
//! class extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::di::{StartupInitializationFailure, StartupService, StartupServiceType};
use rayx_app::services::navigation::NavigationService;
use rayx_app::{AppBuilder, ServiceAppBuilderExt as _, ServiceProvider};
use rayx_app_nsh::ts::{RayXAppBuilder, record};

use crate::nsh_location::NshLocationSynchronizer;
use crate::options::{NAVIGATION_OPTIONS_SECTION, NavigationOptions};
use crate::options_ts::NavigationOptions as TsNavigationOptions;
use crate::service::{RayxNavigation, Starting};
use crate::synchronizer::NavigationLocationSynchronizer;

/// Registers RayX's navigation service on an app builder.
pub trait NavigationAppBuilderExt: AppBuilder {
    /// Registers RayX's navigation service as the app's `dyn NavigationService`, replacing any
    /// other: a startup service that goes to the address the app started at (a deep link, the
    /// address bar) or its default route when the app starts, shows its location as NSH's app
    /// address and follows the platform's moves of it. Its [`NavigationOptions`] bind from
    /// [`NAVIGATION_OPTIONS_SECTION`], then `configure` changes them in code (.NET's
    /// `Bind(..).Configure(..)`); invalid routes fail startup.
    ///
    /// Call it once; more steps go through `configure_options::<NavigationOptions>(..)`.
    fn add_navigation(
        self,
        configure: impl Fn(&mut NavigationOptions) + Send + Sync + 'static,
    ) -> Self {
        add_navigation_with(self, configure, |_| {
            Arc::new(NshLocationSynchronizer::new()) as Arc<dyn NavigationLocationSynchronizer>
        })
    }
}

impl<T> NavigationAppBuilderExt for T where T: AppBuilder {}

/// `add_navigation`, showing the location through the synchronizer `synchronizer` makes.
pub(crate) fn add_navigation_with<B: AppBuilder>(
    mut builder: B,
    configure: impl Fn(&mut NavigationOptions) + Send + Sync + 'static,
    synchronizer: impl Fn(&ServiceProvider) -> Arc<dyn NavigationLocationSynchronizer>
    + Send
    + Sync
    + 'static,
) -> B {
    builder = builder.add_options::<NavigationOptions>(|options| {
        options
            .bind(NAVIGATION_OPTIONS_SECTION)
            .configure(configure)
            .validate(
                |options| RayxNavigation::check(options).is_ok(),
                "the navigation's routes are invalid",
            )
            .validate_on_start();
    });
    let services = builder.services_mut();
    services.replace_service_factory_as::<RayxNavigation>(move |services| {
        let options = services
            .options::<NavigationOptions>()
            .unwrap_or_else(|error| panic!("the navigation's options are invalid: {error}"))
            .cloned();
        Arc::new(
            RayxNavigation::new(options, synchronizer(services))
                .unwrap_or_else(|error| panic!("the navigation's routes are invalid: {error}")),
        )
    });
    // Replaced before the startup service is added: replacing `dyn NavigationService` drops the
    // startup service keyed by it, which `Starting` is.
    services.replace_service_factory_as::<dyn NavigationService>(|services| {
        services
            .required_service::<RayxNavigation>()
            .unwrap_or_else(|error| panic!("RayX's navigation service is registered: {error}"))
    });
    services.add_startup_resolved_async(
        StartupServiceType::of::<dyn NavigationService>(),
        |context| {
            let navigation = context.services().required_service::<RayxNavigation>();
            Box::pin(async move {
                let navigation = navigation.map_err(|error| StartupInitializationFailure {
                    service: StartupServiceType::of::<dyn NavigationService>(),
                    message: error.to_string(),
                    timed_out: false,
                })?;
                Ok(Arc::new(Starting(navigation)) as StartupService)
            })
        },
    );
    builder
}

/// The methods `rayx_services_navigation` adds to TS's `RayXAppBuilder`.
pub struct NavigationBuilder;

#[class_extension(RayXAppBuilder)]
impl NavigationBuilder {
    /// Registers the app's navigation service; `configure` adds its routes
    /// (`options.addRoute("/projects/{id}")`) after configuration's options.
    pub fn add_navigation(
        this: Instance<RayXAppBuilder>,
        #[args(options)] configure: Option<Callback<Instance<TsNavigationOptions>>>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let routes = record(
            TsNavigationOptions::new(),
            configure,
            TsNavigationOptions::take,
        )?;
        RayXAppBuilder::extend_with(this, move |builder| {
            builder.add_navigation(move |options| {
                for route in &routes {
                    TsNavigationOptions::apply(route, options);
                }
            })
        })
    }
}
