//! RayX's core: dependency injection, configuration, options and the app builder, with the API
//! of .NET's `Microsoft.Extensions` (RayX's `rayx_runtime::di`, `configuration`, `options`,
//! `services::app`), without NSH or GPUI. It builds for every target, wasm32 included.
//!
//! - [`di`]: the [`ServiceProvider`] with singleton, scoped and transient services, scopes and
//!   async startup services.
//! - [`configuration`]: sources merged into one [`Configuration`], read by path.
//! - [`options`]: typed options made by a pipeline (bind, configure, post-configure, validate),
//!   read once, per scope or as they change.
//! - [`services::app`]: [`RayXAppBuilder`] and the [`AppBuilder`] traits service crates extend.
//!
//! Porting from RayX changes `use` paths, plus `RayXService` -> [`AppService`]:
//!
//! | RayX | rayx_core |
//! | --- | --- |
//! | `rayx::di::*`, `rayx_runtime::di::*` | `rayx_core::di::*` |
//! | `rayx::services::app::{AppBuilder, ServiceAppBuilderExt}` | `rayx_core::services::app::*` |
//! | `rayx::prelude::{ConfigurationBuilder, ServiceProvider, RayXService}` | `rayx_core::prelude::{.., AppService}` |
//! | `rayx_runtime::options::*`, `configuration::*` | `rayx_core::options::*`, `configuration::*` |
//!
//! RayX's options plans (`options`, `configure_options` with a finalizer, `options_value`) are
//! .NET's options model now: `services.add_options::<T>().bind(..).configure(..).validate(..)`.
//! The most used items are also re-exported at the crate root.
//!
//! Not ported from RayX (GPUI- or platform-bound, or unused by its callers):
//! `despatma`'s `dependency_container` with `RayxBaseModule`/`RayxEmptyService`
//! (so `ServiceProvider::new()` takes no module), `AppContext`,
//! `GpuxServiceAppExt`, the GPUI startup phase (`GpuxStartupContext`,
//! `initialize_gpux`, `gpux_initialization_depends_on`), the platform snapshot in
//! `StartupInitializationContext`, file, environment and app-settings configuration sources
//! (`add_file`, `add_environment`, `add_app_settings`: a [`ConfigurationSource`] of the
//! platform's brings them) and the `rayx_diagnostics` feature.
//!
//! [`ConfigurationSource`]: configuration::ConfigurationSource

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

// Lets `#[derive(AppService)]` name `::rayx_core` from inside this crate too.
extern crate self as rayx_core;

mod change;
pub mod configuration;
pub mod di;
pub mod options;
pub mod services;

pub use change::ChangeSubscription;
pub use configuration::{Configuration, ConfigurationBuilder, ConfigurationError};
pub use di::{
    AppService, ServiceLifetime, ServiceProvider, ServiceResolutionError, StartupRegistry,
};
pub use options::{Options, OptionsBuilder, OptionsError, OptionsMonitor, OptionsSnapshot};
pub use services::app::{
    AppBuilder, ConfigurationAppBuilderExt, RayXAppBuilder, ServiceAppBuilderExt,
};

pub mod prelude {
    pub use crate::change::ChangeSubscription;
    pub use crate::configuration::{
        Configuration, ConfigurationBuilder, ConfigurationError, ConfigurationSection,
        ConfigurationSource,
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
    pub use crate::options::{
        Options, OptionsBuilder, OptionsError, OptionsMonitor, OptionsSnapshot,
    };
    pub use crate::services::app::{
        AppBuilder, ConfigurationAppBuilderExt, RayXAppBuilder, ServiceAppBuilderExt,
    };
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
