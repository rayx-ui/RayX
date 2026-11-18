//! The core's configuration ([`rayx_app::configuration`]), re-exported, with the app's bundled
//! `appsettings.json` and TS's options and configuration builder beside it.

#[doc(no_inline)]
pub use rayx_app::configuration::*;

pub(crate) mod configuration_builder_ts;
pub(crate) mod options_ts;
pub(crate) mod settings;
