//! `add_platform`: in Rust ([`PlatformAppBuilderExt`]) and in TS ([`PlatformBuilder`], a class
//! extension of the host's `RayXAppBuilder`).

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app::AppBuilder;
use rayx_app::services::platform::PlatformInfoService;
use rayx_app_nsh::ts::RayXAppBuilder;

use crate::service::NshPlatform;

/// Registers RayX's platform service on an app builder.
pub trait PlatformAppBuilderExt: AppBuilder {
    /// Registers RayX's platform service as the app's `dyn PlatformInfoService`, replacing any
    /// other: a startup service that publishes NSH's environment when the app starts and keeps
    /// it current from then on.
    fn add_platform(mut self) -> Self {
        let platform = NshPlatform::new();
        let services = self.services_mut();
        // Replacing drops the startup service keyed `dyn PlatformInfoService` (an earlier one's),
        // so the service's own startup registration, under that key, comes after.
        let registered = Arc::clone(&platform);
        services.replace_service_factory_as::<dyn PlatformInfoService>(move |_| {
            Arc::clone(&registered) as Arc<dyn PlatformInfoService>
        });
        services.add_startup_singleton::<NshPlatform>(platform);
        self
    }
}

impl<T> PlatformAppBuilderExt for T where T: AppBuilder {}

/// The methods `rayx_services_platform` adds to TS's `RayXAppBuilder`.
pub struct PlatformBuilder;

#[class_extension(RayXAppBuilder)]
impl PlatformBuilder {
    /// Registers RayX's platform service (the device and the user's settings, kept current).
    pub fn add_platform(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, PlatformAppBuilderExt::add_platform)
    }
}
