//! RayX's platform service on NSH: a [`StateCell`] of the platform's snapshot that its startup
//! initialization fills from NSH's environment and keeps current from then on.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::prelude::{Owner, effect, untracked};
use rayx_app::StateCell;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app::services::platform::{PlatformInfoService, PlatformSnapshot};

use crate::snapshot::{ask_features, platform_snapshot};

/// The device and the user's settings, as NSH's environment reports them on the surface whose
/// provider holds this service.
pub(crate) struct NshPlatform {
    platform: StateCell<PlatformSnapshot>,
    following: AtomicBool,
}

impl NshPlatform {
    /// A service at the default snapshot until it initializes.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            platform: StateCell::new(PlatformSnapshot::default()),
            following: AtomicBool::new(false),
        })
    }

    /// Publishes NSH's environment now and follows it for the surface's life: every change of
    /// the device and the user's settings, and what the platform answers of web views and
    /// speech. On the engine thread, where NSH runs Rust code (the startup run).
    fn follow(&self) {
        if self.following.swap(true, Ordering::SeqCst) {
            return;
        }
        let platform = self.platform.clone();
        platform.publish(untracked(platform_snapshot));
        Owner::detached().run(move || {
            ask_features();
            effect(move || {
                let snapshot = platform_snapshot();
                if *platform.get() != snapshot {
                    platform.publish(snapshot);
                }
            });
        });
    }
}

impl PlatformInfoService for NshPlatform {
    fn platform(&self) -> &StateCell<PlatformSnapshot> {
        &self.platform
    }
}

impl AsyncInitializable for NshPlatform {
    fn is_initialized(&self) -> bool {
        self.following.load(Ordering::SeqCst)
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.follow();
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for NshPlatform {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn PlatformInfoService>())
    }
}
