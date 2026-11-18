//! `add_platform`: in Rust ([`PlatformAppBuilderExt`]) and in TS ([`PlatformBuilder`], a class
//! extension of the host's `RayXAppBuilder`).

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
        // Registered as `dyn PlatformInfoService` and a startup service registered as it: a
        // later registration of `dyn PlatformInfoService` (the app's own) removes both.
        self.services_mut()
            .add_startup_singleton_for::<dyn PlatformInfoService, _>(NshPlatform::new());
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
