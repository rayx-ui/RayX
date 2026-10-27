use super::PlatformInfoService;
use rayx_core::AppBuilder;

/// Registers the [`PlatformInfoService`] on an app builder.
pub trait PlatformAppBuilderExt: AppBuilder {
    /// Registers the app's [`PlatformInfoService`] unless one is registered. It holds a default
    /// desktop snapshot until the UI layer reports the platform's (`rayx_app` does from NSH).
    fn add_platform(mut self) -> Self {
        self.services_mut()
            .try_add_service_factory::<PlatformInfoService>(|_| PlatformInfoService::default());
        self
    }

    /// Registers `service` as the app's [`PlatformInfoService`], replacing any other.
    fn with_platform_service(mut self, service: PlatformInfoService) -> Self {
        self.services_mut().replace_service(service);
        self
    }
}

impl<T> PlatformAppBuilderExt for T where T: AppBuilder {}
