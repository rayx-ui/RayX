//! Dependency injection, options and an app builder with RayX's DI API
//! (`rayx_runtime::di`, `options`, `configuration`, `services::app`), without GPUI.
//!
//! Porting from RayX changes only `use` paths, plus `RayXService` -> [`AppService`]:
//!
//! | RayX | rayx_core |
//! | --- | --- |
//! | `rayx::di::*`, `rayx_runtime::di::*` | `rayx_core::di::*` |
//! | `rayx::services::app::{AppBuilder, ServiceAppBuilderExt}` | `rayx_core::services::app::*` |
//! | `rayx::prelude::{ConfigurationBuilder, ServiceProvider, RayXService}` | `rayx_core::prelude::{.., AppService}` |
//! | `rayx_runtime::options::*`, `configuration::*` | `rayx_core::options::*`, `configuration::*` |
//!
//! The most used items are also re-exported at the crate root.
//!
//! Not ported from RayX (GPUI- or platform-bound, or unused by its callers):
//! `despatma`'s `dependency_container` with `RayxBaseModule`/`RayxEmptyService`
//! (so `ServiceProvider::new()` takes no module), `AppContext`,
//! `GpuxServiceAppExt`, the GPUI startup phase (`GpuxStartupContext`,
//! `initialize_gpux`, `gpux_initialization_depends_on`), the platform snapshot in
//! `StartupInitializationContext`, configuration sources (`add_file`,
//! `add_environment`, `add_app_settings`, `add_source`, `ConfigurationRoot`,
//! `AppContentContext`) and the `rayx_diagnostics` feature.

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

// Lets `#[derive(AppService)]` name `::rayx_core` from inside this crate too.
extern crate self as rayx_core;

pub mod configuration;
pub mod di;
pub mod options;
pub mod services;

pub use configuration::{ConfigurationBuilder, ConfigurationError};
pub use di::{
    AppService, ServiceLifetime, ServiceProvider, ServiceResolutionError, StartupRegistry,
};
pub use options::{Options, OptionsError, OptionsRegistry};
pub use services::app::{AppBuilder, ConfigurationAppBuilderExt, ServiceAppBuilderExt};

pub mod prelude {
    pub use crate::configuration::{
        ConfigurationBuildResult, ConfigurationBuilder, ConfigurationError, OptionsFinalizeError,
        OptionsFinalizeStage,
    };
    pub use crate::di::{
        AppScope, AppService, AsyncInitializable, DIReady, DiReady, IntoServiceArc,
        IntoSingletonService, ServiceLifetime, ServiceProvider, ServiceRegistry,
        ServiceResolutionError, ShutdownService, StartupInitializable,
        StartupInitializationContext, StartupInitializationError, StartupInitializationFailure,
        StartupInitializationFailures, StartupInitializationPhase, StartupInitializationResult,
        StartupInitializationState, StartupRegistry, StartupServiceError, StartupServiceType,
        ViewScope, WindowScope, default_provider,
    };
    pub use crate::options::{Options, OptionsError, OptionsRegistry};
    pub use crate::services::app::{AppBuilder, ConfigurationAppBuilderExt, ServiceAppBuilderExt};
}

#[cfg(test)]
mod tests {
    use crate::{AppService, ServiceProvider};

    #[derive(AppService)]
    struct LocalService;

    #[test]
    fn di_derive_resolves_inside_the_defining_crate() {
        let provider = ServiceProvider::default().with_singleton(LocalService);
        assert!(provider.required_service::<LocalService>().is_ok());
    }
}
