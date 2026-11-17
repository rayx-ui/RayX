//! RayX's application configuration: the settings from the configuration's `RayX:Application`
//! section and code, following the configuration as it reloads and the platform's motion
//! setting, published as an [`ApplicationConfigurationSnapshot`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};

use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_app::services::application_configuration::{
    ApplicationConfiguration, ApplicationConfigurationError, ApplicationConfigurationService,
    ApplicationConfigurationSnapshot,
};
use rayx_app::services::platform::PlatformInfoService;
use rayx_app::{ChangeSubscription, StateCell};

/// RayX's application configuration: the app's settings (all but the startup-only name and
/// description change with the configuration) and whether the platform asks for less motion,
/// which it follows from the app's `dyn PlatformInfoService`.
pub(crate) struct RayxConfiguration {
    /// The snapshot, changed under its lock and published outside it.
    state: Mutex<ApplicationConfigurationSnapshot>,
    configuration: StateCell<ApplicationConfigurationSnapshot>,
    /// Held by the thread publishing the snapshot ([`RayxConfiguration::publish_latest`]).
    publishing: Mutex<()>,
    /// A change was made since the publishing thread read the snapshot.
    stale: AtomicBool,
    /// The platform's changes, heard for as long as the service lives.
    following: Mutex<Option<ChangeSubscription>>,
}

impl RayxConfiguration {
    /// A service over `options`, following `platform`'s motion setting from now on.
    pub(crate) fn new(
        options: ApplicationConfiguration,
        platform: &Arc<dyn PlatformInfoService>,
    ) -> Result<Arc<Self>, ApplicationConfigurationError> {
        options
            .validate()
            .map_err(ApplicationConfigurationError::InvalidOptions)?;
        let snapshot = ApplicationConfigurationSnapshot {
            options,
            platform_reduced_motion: platform.platform().get().reduced_motion,
            revision: 0,
        };
        let service = Arc::new(Self {
            configuration: StateCell::new(snapshot.clone()),
            state: Mutex::new(snapshot),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            following: Mutex::default(),
        });
        let weak = Arc::downgrade(&service);
        let subscription = platform.platform().subscribe(move |snapshot| {
            if let Some(service) = weak.upgrade() {
                service.follow_platform(snapshot.reduced_motion);
            }
        });
        *service
            .following
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(subscription);
        // A change between the first read and the subscription.
        service.follow_platform(platform.platform().get().reduced_motion);
        Ok(service)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ApplicationConfigurationSnapshot> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes the platform's motion setting.
    fn follow_platform(&self, reduced_motion: bool) {
        {
            let mut state = self.lock();
            if state.platform_reduced_motion == reduced_motion {
                return;
            }
            state.platform_reduced_motion = reduced_motion;
            state.revision = state.revision.saturating_add(1);
        }
        self.publish_latest();
    }

    /// Replaces the settings (not their startup-only name and description); whether they
    /// changed.
    pub(crate) fn replace(
        &self,
        options: ApplicationConfiguration,
    ) -> Result<bool, ApplicationConfigurationError> {
        options
            .validate()
            .map_err(ApplicationConfigurationError::InvalidOptions)?;
        {
            let mut state = self.lock();
            if state.options.name != options.name {
                return Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
                    "name",
                ));
            }
            if state.options.description != options.description {
                return Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
                    "description",
                ));
            }
            if state.options == options {
                return Ok(false);
            }
            state.options = options;
            state.revision = state.revision.saturating_add(1);
        }
        self.publish_latest();
        Ok(true)
    }

    /// Publishes the latest snapshot, outside the snapshot's lock (a subscriber may call the
    /// service). One thread publishes at a time and reads the snapshot when it does, so the
    /// published snapshot is never older than one published before it: a change made while
    /// another thread (or a subscriber of this one) publishes is published right after, by the
    /// publishing thread, once it finds the snapshot stale.
    fn publish_latest(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let snapshot = self.lock().clone();
            if self.configuration.get().revision != snapshot.revision {
                self.configuration.publish(snapshot);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }
}

impl ApplicationConfigurationService for RayxConfiguration {
    fn configuration(&self) -> &StateCell<ApplicationConfigurationSnapshot> {
        &self.configuration
    }

    fn replace_options(
        &self,
        options: ApplicationConfiguration,
    ) -> BoxFuture<'_, Result<bool, ApplicationConfigurationError>> {
        futures::future::ready(self.replace(options)).boxed()
    }
}
