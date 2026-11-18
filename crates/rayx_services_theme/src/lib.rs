//! RayX's theme service on NSH: RayX's implementation of
//! [`rayx_app::services::theme::ThemeService`]. On each surface it picks the active theme
//! document's modifier contexts (`set_context`: `accent`, `density`, `os`, …) and keeps the
//! light/dark modifier in step with the color scheme shown, as the app's
//! `dyn PlatformInfoService` (required, never registered: `add_platform()` from
//! `rayx_services_platform`, or the app's own) reports it, or with the app's choice
//! (`set_color_mode`). Its options, [`ThemeOptions`], bind from the configuration's
//! `RayX:Services:Theme` section ([`THEME_CONFIG_PATH`]) and code: the light/dark modifier and
//! its starting choice, and the contexts to pick at start.
//!
//! NSH owns the selection: a snapshot's `selected` is what NSH shows, published once a pick took
//! effect. Density, OS and contrast are not followed from the platform: an app picks them with
//! `set_context`. Without the service, a theme shows its light/dark modifier's default (RayX's
//! theme: `light`).
//!
//! - Rust: `RayXAppBuilder::new().add_theme(|options| { .. })` ([`ThemeAppBuilderExt`]), then
//!   `services.required_service::<dyn ThemeService>()`.
//! - TS: `new RayXAppBuilder().addTheme((options) => { .. })`, with the class [`ThemeService`]
//!   over whichever `dyn ThemeService` is registered (the contract's token).
//! - The app's set: [`register`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod nsh;
mod options;
mod options_ts;
mod service;
mod theme_ts;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::theme::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{ThemeAppBuilderExt, ThemeBuilder};
pub use options::{ColorModeOptions, THEME_CONFIG_PATH, ThemeOptions};
pub use options_ts::ThemeOptions as TsThemeOptions;
pub use theme_ts::ThemeService;

rayx_app_nsh::contract_nsh!(rayx_app::services::theme::ThemeService as ThemeContract);

/// The light/dark choice in TS.
pub const COLOR_MODE_CHOICE: TsAlias = TsAlias {
    name: "ColorModeChoice",
    ts: "\"System\" | \"Light\" | \"Dark\"",
    doc: "Whether the theme's light/dark modifier follows the color scheme shown (`System`) or shows its light or dark context: the JSON of Rust's `ColorModeChoice`.",
};

/// One of the theme document's modifiers in TS.
pub const THEME_MODIFIER: TsAlias = TsAlias {
    name: "ThemeModifier",
    ts: "{ name?: string; contexts?: string[]; default?: string; selected?: string }",
    doc: "One of the active theme document's modifiers: its contexts, its default and the context it shows: the JSON of Rust's `ThemeModifier` (a field left out is its default).",
};

/// The theme's snapshot in TS.
pub const THEME_SNAPSHOT: TsAlias = TsAlias {
    name: "ThemeSnapshot",
    ts: "{ modifiers?: ThemeModifier[]; color_mode?: ColorModeChoice; revision?: number }",
    doc: "The active theme document's modifiers with the context each shows, the light/dark choice and a revision counting the snapshots: the JSON of Rust's `ThemeSnapshot` (a field left out is its default).",
};

/// Adds the theme service's TS side to an app's set: its class [`ThemeService`], its options
/// class (`ThemeOptions` in TS), the contract a TS implementation follows (the TS interface
/// `ThemeServiceContract`, [`ThemeContract`] in Rust, whose token is the class) and
/// `builder.addTheme(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(COLOR_MODE_CHOICE)
        .ts_alias(THEME_MODIFIER)
        .ts_alias(THEME_SNAPSHOT)
        .class::<TsThemeOptions>()
        .contract_class::<ThemeContract, ThemeService>()
        .class_extension::<ThemeBuilder>();
}
