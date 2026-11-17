//! RayX's i18n service on NSH: the app's languages and
//! [MessageFormat 2](https://www.unicode.org/reports/tr35/tr35-messageFormat.html) messages (the
//! asset catalog's `locales/`, looked up for the user's languages with fallbacks), published by
//! RayX's implementation of [`rayx_app::services::i18n::I18nService`], and formatted with
//! numbers, dates and plurals for the user's locale by RayX's
//! [`rayx_app::services::i18n::MessageFormatter`], through [`dxn_i18n`] and [`dxn_intl`]. It
//! reads the messages through the app's `dyn AssetService` and follows the user's languages and
//! clock as the app's `dyn PlatformInfoService` reports them, both of which it requires.
//!
//! - Rust: `RayXAppBuilder::new().add_i18n(|options| { .. })` ([`I18nAppBuilderExt`]), or
//!   `add_message_formatter()` over an i18n service of the app's own; [`tr!`] in a Rust
//!   component (`text(move || tr!("cart.items", count))`, following the language), and
//!   [`checked::tr!`](checked::tr), checked against the app's messages when it builds. NSH's
//!   locale follows the language the app picks.
//! - TS: `new RayXAppBuilder().addI18n((options) => { .. })`, with the class [`I18nService`]
//!   over whichever `dyn I18nService` is registered (the contract's token), formatting
//!   synchronously through the formatter.
//! - The app's set: [`register`].
//!
//! Formatting is RayX's for any `I18nService` implementation, a TS one included: the service
//! publishes its messages as resource bundles, which the formatter compiles per language and
//! compiles again on each new version of the service's state.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod formatter;
mod i18n_ts;
mod locale;
mod options;
mod options_ts;
mod service;
mod translate;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::i18n::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{I18nAppBuilderExt, I18nBuilder};
pub use dxn_i18n::{Args, Translate, Value, args};
pub use i18n_ts::{DateFormat, DateLength, I18nService, NumberFormat, NumberKind, TimeDetail};
pub use options::{I18N_CONFIG_PATH, I18nOptions, MissingMessagePolicy};
pub use options_ts::I18nOptions as TsI18nOptions;
#[doc(hidden)]
pub use translate::__private;
pub use translate::{checked, translate};

rayx_app_nsh::contract_nsh!(rayx_app::services::i18n::I18nService as I18nContract);

/// The i18n state in TS.
pub const I18N_STATE: TsAlias = TsAlias {
    name: "I18nState",
    ts: "Record<string, unknown>",
    doc: "The languages, locale, time zone and messages: the JSON of Rust's `I18nState`.",
};

/// Adds the i18n service's TS side to an app's set: its class [`I18nService`], its options class
/// (`I18nOptions` in TS), the contract a TS implementation follows (the TS interface
/// `I18nServiceContract`, [`I18nContract`] in Rust, whose token is the class) and
/// `builder.addI18n(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(I18N_STATE)
        .class::<TsI18nOptions>()
        .contract_class::<I18nContract, I18nService>()
        .class_extension::<I18nBuilder>();
}
