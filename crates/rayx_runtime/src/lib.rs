//! RayX's app services, without NSH or GPUI (ported from RayX's `rayx_runtime::services`). Each
//! service is registered on a [`RayXAppBuilder`](rayx_app::RayXAppBuilder) through an
//! `AppBuilder` extension trait (`add_input_map(..)`, `add_selection()`, …) and reads its options
//! from the configuration under `RayX:Services:<Name>` through [`rayx_app`]'s options model.
//! What a service needs from the platform (reading the app's files, registering fonts, the
//! device's state) is a trait here that the NSH layer, `rayx_app_nsh`, implements. It builds for
//! every target, wasm32 included.
//!
//! `docs/services.md` describes each service and how it is registered.

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod services;

/// The services' builder extensions: `use rayx_runtime::prelude::*;` to register them.
pub mod prelude {
    pub use crate::services::application_configuration::ApplicationConfigurationAppBuilderExt;
}
