//! The app builder ([`RayXAppBuilder`]) and the traits service crates extend it through
//! ([`AppBuilder`], with [`ServiceAppBuilderExt`], [`ConfigurationAppBuilderExt`] and
//! [`ReadyAppBuilderExt`]). Starting the app is the NSH host's (`rayx_app_nsh`).

mod builder;
mod configuration_ext;
mod rayx_app_builder;
mod ready;
mod service_ext;

pub use builder::AppBuilder;
pub use configuration_ext::ConfigurationAppBuilderExt;
pub use rayx_app_builder::{AppBuildError, RayXAppBuilder};
pub use ready::{ReadyAppBuilderExt, ReadyContext, ReadyError, ReadyHooks, ReadyResult};
pub use service_ext::ServiceAppBuilderExt;
