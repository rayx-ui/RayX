//! `add_selection`: in Rust ([`SelectionAppBuilderExt`]) and in TS ([`SelectionBuilder`], a
//! class extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder;
use rayx_app::services::navigation::NavigationService;
use rayx_app::services::selection::SelectionService;
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::service::RayxSelection;

/// What an app registering RayX's selection service without a navigation service is told.
const NAVIGATION_HINT: &str = "add_navigation(..) (addNavigation in TS) from \
                               rayx_services_navigation, or register your own dyn \
                               NavigationService";

/// Registers RayX's selection service on an app builder.
pub trait SelectionAppBuilderExt: AppBuilder {
    /// Registers RayX's selection service as the app's `dyn SelectionService`, replacing any
    /// other. It clears on every route change of the app's `dyn NavigationService`, which it
    /// requires (registered before or after it): without one, the app does not build.
    fn add_selection(mut self) -> Self {
        let services = self.services_mut();
        services.require_for::<dyn SelectionService, dyn NavigationService>(NAVIGATION_HINT);
        services.replace_service_factory_as::<dyn SelectionService>(|services| {
            let navigation = services
                .required_service::<dyn NavigationService>()
                .unwrap_or_else(|error| {
                    panic!("the selection service needs {NAVIGATION_HINT}: {error}")
                });
            Arc::new(RayxSelection::new(navigation.as_ref())) as Arc<dyn SelectionService>
        });
        self
    }
}

impl<T> SelectionAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_selection` adds to TS's `RayXAppBuilder`.
pub struct SelectionBuilder;

#[class_extension(RayXAppBuilder)]
impl SelectionBuilder {
    /// Registers the app's selection service.
    pub fn add_selection(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, SelectionAppBuilderExt::add_selection)
    }
}
