//! RayX's application configuration on NSH: RayX's implementation of
//! [`rayx_app::services::application_configuration::ApplicationConfigurationService`], the app's
//! own settings ([`ApplicationConfiguration`]: name, motion policy, programmatic scrolling,
//! tooltip, hover-card and notification behaviour, layout direction) bound from the
//! configuration's `RayX:Application` section ([`APPLICATION_CONFIGURATION_PATH`]) and code,
//! following the configuration as it reloads, and the user's motion setting as the app's
//! `dyn PlatformInfoService` (required, never registered: `add_platform()` from
//! `rayx_services_platform`, or the app's own) reports it.
//!
//! - Rust: `RayXAppBuilder::new().add_application_configuration(|settings| { .. })`
//!   ([`ApplicationConfigurationAppBuilderExt`]), then
//!   `services.required_service::<dyn ApplicationConfigurationService>()`.
//! - TS: `new RayXAppBuilder().addApplicationConfiguration((options) => { .. })`, with the class
//!   [`ApplicationConfigurationService`] over whichever `dyn ApplicationConfigurationService` is
//!   registered (the contract's token).
//! - The app's set: [`register`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod application_configuration_ts;
mod builder;
mod options_ts;
mod service;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::application_configuration::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use application_configuration_ts::ApplicationConfigurationService;
pub use builder::{ApplicationConfigurationAppBuilderExt, ApplicationConfigurationBuilder};
pub use options_ts::ApplicationConfigurationOptions;

/// The configuration section RayX's [`ApplicationConfiguration`] binds from.
pub const APPLICATION_CONFIGURATION_PATH: &str = "RayX:Application";

rayx_app_nsh::contract_nsh!(
    rayx_app::services::application_configuration::ApplicationConfigurationService
        as ApplicationConfigurationContract
);

/// The app's settings in TS.
pub const APPLICATION_CONFIGURATION: TsAlias = TsAlias {
    name: "ApplicationConfiguration",
    ts: "{ name?: string; description?: string; motion?: { policy?: \"System\" | \"Reduced\" }; \
         scrolling?: { programmatic?: \"Instant\" | \"Smooth\"; smooth_duration_ms?: number }; \
         tooltips?: { show_delay_ms?: number; grace_period_ms?: number }; \
         hover_cards?: { open_delay_ms?: number; close_delay_ms?: number }; \
         notifications?: { auto_hide_ms?: number; placement?: string; \
         margins?: { top?: number; right?: number; bottom?: number; left?: number }; \
         max_items?: number }; layout?: { direction?: \"ltr\" | \"rtl\" } }",
    doc: "The app's settings: the JSON of Rust's `ApplicationConfiguration` (a field left out is its default).",
};

/// The application configuration's snapshot in TS.
pub const APPLICATION_CONFIGURATION_SNAPSHOT: TsAlias = TsAlias {
    name: "ApplicationConfigurationSnapshot",
    ts: "{ options?: ApplicationConfiguration; platform_reduced_motion?: boolean; revision?: number }",
    doc: "The settings, whether the platform asks for less motion and a revision counting their changes: the JSON of Rust's `ApplicationConfigurationSnapshot` (a field left out is its default).",
};

/// Adds the application configuration's TS side to an app's set: its class
/// [`ApplicationConfigurationService`], its options class [`ApplicationConfigurationOptions`],
/// the contract a TS implementation follows (the TS interface
/// `ApplicationConfigurationServiceContract`, [`ApplicationConfigurationContract`] in Rust, whose
/// token is the class) and `builder.addApplicationConfiguration(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(APPLICATION_CONFIGURATION)
        .ts_alias(APPLICATION_CONFIGURATION_SNAPSHOT)
        .class::<ApplicationConfigurationOptions>()
        .contract_class::<ApplicationConfigurationContract, ApplicationConfigurationService>()
        .class_extension::<ApplicationConfigurationBuilder>();
}
