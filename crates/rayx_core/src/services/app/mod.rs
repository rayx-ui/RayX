//! The app builder ([`RayXAppBuilder`]) and the traits service crates extend it through (RayX's
//! `rayx_runtime::services::app`).
//!
//! RayX's `AppContext` (a GPUI global with a platform snapshot) and
//! `GpuxServiceAppExt` are GPUI-specific and live in the NSH layer instead.

mod builder;
mod configuration_ext;
mod rayx_app_builder;
mod service_ext;

pub use builder::AppBuilder;
pub use configuration_ext::ConfigurationAppBuilderExt;
pub use rayx_app_builder::RayXAppBuilder;
pub use service_ext::ServiceAppBuilderExt;
