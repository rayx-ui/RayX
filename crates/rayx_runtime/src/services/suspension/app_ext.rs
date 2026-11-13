use std::sync::Arc;

use super::{SuspensionHost, SuspensionService};
use rayx_core::AppBuilder;

/// Registers the [`SuspensionService`] on an app builder.
pub trait SuspensionAppBuilderExt: AppBuilder {
    /// Registers the app's [`SuspensionService`] unless one is registered; the UI layer
    /// publishes the app's lifecycle to it (`rayx_app` does from NSH) and quits through the
    /// app's `dyn SuspensionHost`, when one is registered (`rayx_app` registers NSH's).
    fn add_suspension(mut self) -> Self {
        self.services_mut()
            .try_add_service_factory::<SuspensionService>(|services| {
                match services.service::<dyn SuspensionHost>() {
                    Some(host) => SuspensionService::with_host(host),
                    None => SuspensionService::new(),
                }
            });
        self
    }

    /// Registers `host` as the app's `dyn SuspensionHost`, replacing any other:
    /// [`add_suspension`](Self::add_suspension) quits and asks before quitting through it.
    fn with_suspension_host(mut self, host: impl SuspensionHost) -> Self {
        let host: Arc<dyn SuspensionHost> = Arc::new(host);
        self.services_mut()
            .replace_service::<dyn SuspensionHost>(host);
        self
    }

    /// Registers `service` as the app's [`SuspensionService`], replacing any other.
    fn with_suspension_service(mut self, service: SuspensionService) -> Self {
        self.services_mut().replace_service(service);
        self
    }
}

impl<T> SuspensionAppBuilderExt for T where T: AppBuilder {}
