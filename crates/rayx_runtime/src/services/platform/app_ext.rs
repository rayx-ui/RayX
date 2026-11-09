use std::future::Future;

use super::{PlatformInfoService, PlatformSnapshot};
use rayx_core::{AppBuilder, ReadyAppBuilderExt as _, ReadyContext, ReadyResult};

/// Registers the [`PlatformInfoService`] on an app builder.
pub trait PlatformAppBuilderExt: AppBuilder {
    /// Registers the app's [`PlatformInfoService`] unless one is registered. It holds a default
    /// desktop snapshot until the UI layer reports the platform's (`rayx_app` does from NSH,
    /// before the startup services initialize).
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

    /// Runs `hook` once the app is ready when `predicate` holds for the platform's snapshot then
    /// (RayX's `on_ready_when`): `|platform| platform.form_factor.is_handheld()`. Without a
    /// platform service it does not run.
    fn on_ready_when_platform<P, F, Fut>(self, predicate: P, hook: F) -> Self
    where
        P: Fn(&PlatformSnapshot) -> bool + Send + Sync + 'static,
        F: Fn(ReadyContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadyResult> + 'static,
    {
        self.on_ready_when(
            move |services| {
                services
                    .service::<PlatformInfoService>()
                    .is_some_and(|platform| predicate(&platform.current()))
            },
            hook,
        )
    }
}

impl<T> PlatformAppBuilderExt for T where T: AppBuilder {}
