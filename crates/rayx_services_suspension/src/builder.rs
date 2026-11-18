//! `add_suspension`: in Rust ([`SuspensionAppBuilderExt`]) and in TS ([`SuspensionBuilder`], a
//! class extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder;
use rayx_app::services::suspension::SuspensionService;
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::service::{Following, NshSuspension};

/// Registers RayX's suspension service on an app builder.
pub trait SuspensionAppBuilderExt: AppBuilder {
    /// Registers RayX's suspension service as the app's `dyn SuspensionService`, replacing any
    /// other: a startup service that follows NSH's app state, a desktop's window and the
    /// system's wakes from the app's start on, and quits through NSH.
    fn add_suspension(mut self) -> Self {
        let suspension = NshSuspension::new();
        let services = self.services_mut();
        // Registered first: registering `dyn SuspensionService` removes the startup services
        // registered as it, which `Following`, added after, is; so does a later registration
        // of it (the app's own).
        let service = Arc::clone(&suspension);
        services.replace_service_factory_as::<dyn SuspensionService>(move |_| {
            Arc::clone(&service) as Arc<dyn SuspensionService>
        });
        services
            .add_startup_service_for::<dyn SuspensionService, _>(Arc::new(Following(suspension)));
        self
    }
}

impl<T> SuspensionAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_suspension` adds to TS's `RayXAppBuilder`.
pub struct SuspensionBuilder;

#[class_extension(RayXAppBuilder)]
impl SuspensionBuilder {
    /// Registers RayX's suspension service (the app's lifecycle, window and wakes, and
    /// quitting).
    pub fn add_suspension(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, SuspensionAppBuilderExt::add_suspension)
    }
}
