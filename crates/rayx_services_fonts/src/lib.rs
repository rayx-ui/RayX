//! RayX's font service on NSH: the app's fonts registered with NSH (the asset catalog's font
//! files when the app starts, others from their data while it runs, from any thread through the
//! surface's engine runner) and the aliases the app names fonts by, published by RayX's
//! implementation of [`rayx_app::services::fonts::FontService`]. It reads the font files
//! through the app's `dyn AssetService`, which it requires.
//!
//! - Rust: `RayXAppBuilder::new().add_assets(|_| {}).add_fonts(|options| { .. })`
//!   ([`FontAppBuilderExt`]).
//! - TS: `new RayXAppBuilder().addAssets().addFonts((options) => { .. })`, with the class
//!   [`FontService`] over whichever `dyn FontService` is registered.
//! - The app's set: [`register`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod fonts_ts;
mod nsh_registrar;
mod options;
mod options_ts;
mod registrar;
mod service;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::fonts::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{FontAppBuilderExt, FontBuilder};
pub use fonts_ts::FontService;
pub use options::{
    FONT_CONFIG_PATH, FontAlias, FontLoadFallback, FontLoadFallbackKind, FontOptions,
    FontOptionsError,
};
pub use options_ts::{FontAliasOptions, FontOptions as TsFontOptions};

rayx_app_nsh::contract_nsh!(rayx_app::services::fonts::FontService as FontContract);

/// The font state in TS.
pub const FONT_STATE: TsAlias = TsAlias {
    name: "FontState",
    ts: "Record<string, unknown>",
    doc: "The fonts registered and the aliases resolved: the JSON of Rust's `FontState`.",
};

/// Adds the font service's TS side to an app's set: its class [`FontService`], its options
/// classes, the contract a TS implementation follows (the TS interface `FontServiceContract`,
/// [`FontContract`] in Rust) and `builder.addFonts(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(FONT_STATE)
        .class::<TsFontOptions>()
        .class::<FontAliasOptions>()
        .contract_class::<FontContract, FontService>()
        .class_extension::<FontBuilder>();
}

/// RayX's font service over a registrar of the test's own, for the crate's tests and an app's.
#[doc(hidden)]
pub mod testing {
    use std::sync::Arc;

    use rayx_app::AppBuilder;

    use crate::FontOptions;
    pub use crate::registrar::FontRegistrar;

    /// `add_fonts`, registering through `registrar` in place of NSH.
    pub fn add_fonts_with<B: AppBuilder>(
        builder: B,
        configure: impl Fn(&mut FontOptions) + Send + Sync + 'static,
        registrar: Arc<dyn FontRegistrar>,
    ) -> B {
        crate::builder::add_fonts_with(builder, configure, move |_| Arc::clone(&registrar))
    }
}
