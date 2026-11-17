//! The platform: the device and the user's settings ([`PlatformSnapshot`]: OS and platform,
//! form factor, input (a TV's remote included), viewport, safe area, keyboard, appearance,
//! motion, accessibility, language and regional settings, a terminal's grid, what the platform
//! can do, and a desktop's displays and window), published by a [`PlatformInfoService`]
//! implementation and kept current by it.
//!
//! RayX's implementation is `rayx_services_platform`'s (`add_platform()`), which follows NSH's
//! environment; an app may register its own `dyn PlatformInfoService`, in Rust or in TS.

mod snapshot;

use std::future::Future;

pub use snapshot::{
    ColorScheme, DisplaySnapshot, FormFactor, InputCapabilities, Insets, KeyboardCapability,
    LocaleSnapshot, Orientation, PlatformFamily, PlatformFeatures, PlatformRect, PlatformSnapshot,
    PointerCapability, TextDirection, ViewportSnapshot, WindowMode, WindowSnapshot,
};

use crate::published::StateCell;
use crate::{AppBuilder, ReadyAppBuilderExt as _, ReadyContext, ReadyResult};

/// The platform service: the device and the user's settings as the app's surface sees them,
/// published as they change. On NSH each surface has a provider, so the service describes its
/// surface.
#[rayx_app_macros::contract(ts = "PlatformInfoServiceContract")]
pub trait PlatformInfoService {
    /// The device and the user's settings now, and each change of them.
    #[state(ts = "PlatformSnapshot")]
    fn platform(&self) -> &StateCell<PlatformSnapshot>;
}

/// What an app does with the platform as it builds: hooks that run once the app is ready,
/// depending on the platform then.
pub trait PlatformReadyAppBuilderExt: AppBuilder {
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
                    .service::<dyn PlatformInfoService>()
                    .is_some_and(|platform| predicate(&platform.platform().get()))
            },
            hook,
        )
    }
}

impl<T> PlatformReadyAppBuilderExt for T where T: AppBuilder {}
