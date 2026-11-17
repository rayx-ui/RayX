//! The application configuration: the app's own settings ([`ApplicationConfiguration`]: name,
//! motion policy, programmatic scrolling, tooltip, hover-card and notification behaviour, layout
//! direction), published by an [`ApplicationConfigurationService`] implementation as an
//! [`ApplicationConfigurationSnapshot`] with whether the platform asks for less motion. The
//! behaviour they give ([`ApplicationConfigurationSnapshot::layout_direction`],
//! [`ApplicationConfigurationSnapshot::effective_motion`],
//! [`ApplicationConfigurationSnapshot::effective_programmatic_scroll_behavior`]) is a function of
//! the snapshot, so it is synchronous for any implementation.
//!
//! RayX's implementation is `rayx_services_application_configuration`'s
//! (`add_application_configuration(..)`), which binds the settings from the configuration's
//! `RayX:Application` section and follows it and the app's `dyn PlatformInfoService`; an app may
//! register its own, in Rust or in TS.

mod settings;

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use settings::{
    ApplicationConfiguration, HoverCardBehaviorOptions, LayoutBehaviorOptions, LayoutDirection,
    MotionBehaviorOptions, MotionPolicy, NotificationBehaviorOptions, NotificationMargins,
    NotificationPlacement, ProgrammaticScrollBehavior, ScrollingBehaviorOptions,
    TooltipBehaviorOptions,
};

use super::ContractFailure;
use crate::published::StateCell;

/// Why settings were refused.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ApplicationConfigurationError {
    #[error("the application configuration is invalid: {0}")]
    InvalidOptions(&'static str),
    #[error("the application configuration's `{0}` is read at startup only")]
    StartupOnlyOptionChanged(&'static str),
    /// The call failed (a TS implementation threw, the engine is gone).
    #[error("{0}")]
    Failed(ContractFailure),
}

impl From<ContractFailure> for ApplicationConfigurationError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The settings, whether the platform asks for less motion, and a revision that counts their
/// changes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationConfigurationSnapshot {
    pub options: ApplicationConfiguration,
    /// The user's platform setting to reduce motion (RayX's implementation follows the app's
    /// `dyn PlatformInfoService`).
    pub platform_reduced_motion: bool,
    /// Counts the changes of the snapshot.
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

impl ApplicationConfigurationSnapshot {
    /// The layout's reading direction.
    pub fn layout_direction(&self) -> LayoutDirection {
        self.options.layout.direction
    }

    /// Whether motion is reduced: the platform's setting, the app's policy, or `local_request`
    /// (a component's own).
    pub fn effective_motion(&self, local_request: Option<MotionPolicy>) -> EffectiveMotion {
        EffectiveMotion {
            platform_reduced: self.platform_reduced_motion,
            app_reduced: self.options.motion.policy == MotionPolicy::Reduced,
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
        let scrolling = &self.options.scrolling;
        let duration =
            local_duration.unwrap_or_else(|| Duration::from_millis(scrolling.smooth_duration_ms));
        Ok(match local_behavior.unwrap_or(scrolling.programmatic) {
            ProgrammaticScrollBehavior::Instant => ScrollBehavior::Instant,
            ProgrammaticScrollBehavior::Smooth => ScrollBehavior::Smooth(duration),
        })
    }
}

/// The application configuration service: the app's settings and the platform's motion setting,
/// published as [`ApplicationConfigurationSnapshot`], and the command that replaces the
/// settings.
#[rayx_app_macros::contract(ts = "ApplicationConfigurationServiceContract")]
pub trait ApplicationConfigurationService {
    /// The settings now, and each change of them.
    #[state(ts = "ApplicationConfigurationSnapshot")]
    fn configuration(&self) -> &StateCell<ApplicationConfigurationSnapshot>;

    /// Replaces the settings (not their startup-only name and description); whether they
    /// changed.
    #[ts("(options: ApplicationConfiguration): boolean")]
    async fn replace_options(
        &self,
        options: ApplicationConfiguration,
    ) -> Result<bool, ApplicationConfigurationError>;
}

impl dyn ApplicationConfigurationService {
    /// The settings, the platform's motion setting and their revision.
    pub fn snapshot(&self) -> ApplicationConfigurationSnapshot {
        (*self.configuration().get()).clone()
    }

    /// The settings.
    pub fn options(&self) -> ApplicationConfiguration {
        self.configuration().get().options.clone()
    }

    /// The layout's reading direction ([`ApplicationConfigurationSnapshot::layout_direction`]).
    pub fn layout_direction(&self) -> LayoutDirection {
        self.configuration().get().layout_direction()
    }

    /// Whether motion is reduced ([`ApplicationConfigurationSnapshot::effective_motion`]).
    pub fn effective_motion(&self, local_request: Option<MotionPolicy>) -> EffectiveMotion {
        self.configuration().get().effective_motion(local_request)
    }

    /// How a programmatic scroll moves
    /// ([`ApplicationConfigurationSnapshot::effective_programmatic_scroll_behavior`]).
    pub fn effective_programmatic_scroll_behavior(
        &self,
        local_behavior: Option<ProgrammaticScrollBehavior>,
        local_duration: Option<Duration>,
        local_motion: Option<MotionPolicy>,
    ) -> Result<ScrollBehavior, ApplicationConfigurationError> {
        self.configuration()
            .get()
            .effective_programmatic_scroll_behavior(local_behavior, local_duration, local_motion)
    }
}
