use super::SuspensionService;
use rayx_core::AppBuilder;

/// Registers the [`SuspensionService`] on an app builder.
pub trait SuspensionAppBuilderExt: AppBuilder {
    /// Registers the app's [`SuspensionService`] unless one is registered; the UI layer
    /// publishes the app's lifecycle to it (`rayx_app` does from NSH).
    fn add_suspension(mut self) -> Self {
        self.services_mut()
            .try_add_service_factory::<SuspensionService>(|_| SuspensionService::new());
        self
    }

    /// Registers `service` as the app's [`SuspensionService`], replacing any other.
    fn with_suspension_service(mut self, service: SuspensionService) -> Self {
        self.services_mut().replace_service(service);
        self
    }
}

impl<T> SuspensionAppBuilderExt for T where T: AppBuilder {}
