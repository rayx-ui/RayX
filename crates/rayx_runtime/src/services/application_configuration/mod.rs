//! The application configuration: the app's own settings from the configuration's
//! `RayX:Application` section and code (name, motion policy, programmatic scrolling, tooltip,
//! hover-card and notification behaviour, layout direction), following the configuration as it
//! reloads, and the behaviour they give with the platform's settings.

mod app_ext;
mod options;
mod service;

pub use app_ext::ApplicationConfigurationAppBuilderExt;
pub use options::{
    APPLICATION_CONFIGURATION_PATH, ApplicationConfiguration, HoverCardBehaviorOptions,
    LayoutBehaviorOptions, LayoutDirection, MotionBehaviorOptions, MotionPolicy,
    NotificationBehaviorOptions, NotificationMargins, NotificationPlacement,
    ProgrammaticScrollBehavior, ScrollingBehaviorOptions, TooltipBehaviorOptions,
};
pub use service::{
    ApplicationConfigurationError, ApplicationConfigurationService,
    ApplicationConfigurationSnapshot, EffectiveMotion, ScrollBehavior,
};
