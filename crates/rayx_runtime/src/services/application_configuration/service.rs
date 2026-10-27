use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rayx_core::ChangeSubscription;
use rayx_core::change::Listeners;
use rayx_core::di::AppService;

use super::{ApplicationConfiguration, LayoutDirection, MotionPolicy, ProgrammaticScrollBehavior};
use crate::services::platform::PlatformInfoService;

/// Why settings were refused.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ApplicationConfigurationError {
    #[error("the application configuration is invalid: {0}")]
    InvalidOptions(&'static str),
    #[error("the application configuration's `{0}` is read at startup only")]
    StartupOnlyOptionChanged(&'static str),
}

/// The settings, with a revision that counts their changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationConfigurationSnapshot {
    pub options: ApplicationConfiguration,
    pub revision: u64,
}

/// Whether motion is reduced, and who asked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveMotion {
    pub platform_reduced: bool,
    pub app_reduced: bool,
    pub local_reduced: bool,
}

impl EffectiveMotion {
    /// Whether the system, the app or the caller asked for less motion.
    pub fn is_reduced(self) -> bool {
        self.platform_reduced || self.app_reduced || self.local_reduced
    }
}

/// How a programmatic scroll moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollBehavior {
    Instant,
    Smooth(Duration),
}

type ConfigurationListener = dyn Fn(&ApplicationConfigurationSnapshot) + Send + Sync;

/// The app's [`ApplicationConfiguration`], as the configuration's `RayX:Application` section
/// and the app's code set it, changing with the configuration (all but the startup-only name
/// and description), and the behaviour it gives with the platform's settings: effective
/// motion, programmatic scrolling.
#[derive(Clone, AppService)]
pub struct ApplicationConfigurationService {
    platform: PlatformInfoService,
    snapshot: Arc<Mutex<ApplicationConfigurationSnapshot>>,
    listeners: Listeners<ConfigurationListener>,
}

impl ApplicationConfigurationService {
    /// A service over `options`, reading the system's settings from `platform`.
    pub fn new(
        options: ApplicationConfiguration,
        platform: PlatformInfoService,
    ) -> Result<Self, ApplicationConfigurationError> {
        options
            .validate()
            .map_err(ApplicationConfigurationError::InvalidOptions)?;
        Ok(Self {
            platform,
            snapshot: Arc::new(Mutex::new(ApplicationConfigurationSnapshot {
                options,
                revision: 0,
            })),
            listeners: Listeners::default(),
        })
    }

    /// The settings and their revision.
    pub fn snapshot(&self) -> ApplicationConfigurationSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The settings.
    pub fn options(&self) -> ApplicationConfiguration {
        self.snapshot().options
    }

    /// Replaces the settings (not their startup-only name and description); listeners hear it
    /// when they changed. Returns whether they did.
    pub fn replace_options(
        &self,
        options: ApplicationConfiguration,
    ) -> Result<bool, ApplicationConfigurationError> {
        options
            .validate()
            .map_err(ApplicationConfigurationError::InvalidOptions)?;
        let snapshot = {
            let mut snapshot = self.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
            if snapshot.options.name != options.name {
                return Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
                    "name",
                ));
            }
            if snapshot.options.description != options.description {
                return Err(ApplicationConfigurationError::StartupOnlyOptionChanged(
                    "description",
                ));
            }
            if snapshot.options == options {
                return Ok(false);
            }
            snapshot.options = options;
            snapshot.revision = snapshot.revision.saturating_add(1);
            snapshot.clone()
        };
        self.listeners.notify(|listener| listener(&snapshot));
        Ok(true)
    }

    /// Calls `listener` with every change of the settings until the subscription is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(&ApplicationConfigurationSnapshot) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.listeners.add(Arc::new(listener))
    }

    /// The layout's reading direction.
    pub fn layout_direction(&self) -> LayoutDirection {
        self.snapshot().options.layout.direction
    }

    /// Whether motion is reduced: the system's setting, the app's policy, or `local_request`
    /// (a component's own).
    pub fn effective_motion(&self, local_request: Option<MotionPolicy>) -> EffectiveMotion {
        EffectiveMotion {
            platform_reduced: self.platform.current().reduced_motion,
            app_reduced: self.snapshot().options.motion.policy == MotionPolicy::Reduced,
            local_reduced: local_request == Some(MotionPolicy::Reduced),
        }
    }

    /// How a programmatic scroll moves: instant when motion is reduced, else the caller's
    /// behaviour and duration over the app's.
    pub fn effective_programmatic_scroll_behavior(
        &self,
        local_behavior: Option<ProgrammaticScrollBehavior>,
        local_duration: Option<Duration>,
        local_motion: Option<MotionPolicy>,
    ) -> Result<ScrollBehavior, ApplicationConfigurationError> {
        if local_duration.is_some_and(|duration| duration.is_zero()) {
            return Err(ApplicationConfigurationError::InvalidOptions(
                "a smooth scroll's duration must be greater than zero",
            ));
        }
        if self.effective_motion(local_motion).is_reduced() {
            return Ok(ScrollBehavior::Instant);
        }
        let scrolling = self.snapshot().options.scrolling;
        let duration =
            local_duration.unwrap_or_else(|| Duration::from_millis(scrolling.smooth_duration_ms));
        Ok(match local_behavior.unwrap_or(scrolling.programmatic) {
            ProgrammaticScrollBehavior::Instant => ScrollBehavior::Instant,
            ProgrammaticScrollBehavior::Smooth => ScrollBehavior::Smooth(duration),
        })
    }
}
