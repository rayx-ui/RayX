//! RayX's core: dependency injection, configuration, options and the app builder, with the API
//! of .NET's `Microsoft.Extensions` (RayX's `rayx_runtime::di`, `configuration`, `options`,
//! `services::app`), without NSH or GPUI. It builds for every target, wasm32 included.
//!
//! - [`di`]: the [`ServiceProvider`] with singleton, scoped and transient services, scopes and
//!   async startup services.
//! - [`configuration`]: sources merged into one [`Configuration`], read by path.
//! - [`options`]: typed options made by a pipeline (bind, configure, post-configure, validate),
//!   read once, per scope or as they change.
//! - [`published`]: [`StateCell`] and [`EventStream`], the state and events a service's
//!   implementation publishes, read and followed from any thread.
//! - [`services::app`]: [`RayXAppBuilder`] and the [`AppBuilder`] traits service crates extend.
//!
//! Porting from RayX changes `use` paths, plus `RayXService` -> [`AppService`]:
//!
//! | RayX | rayx_app |
//! | --- | --- |
//! | `rayx::di::*`, `rayx_runtime::di::*` | `rayx_app::di::*` |
//! | `rayx::services::app::{AppBuilder, ServiceAppBuilderExt}` | `rayx_app::services::app::*` |
//! | `rayx::prelude::{ConfigurationBuilder, ServiceProvider, RayXService}` | `rayx_app::prelude::{.., AppService}` |
//! | `rayx_runtime::options::*`, `configuration::*` | `rayx_app::options::*`, `configuration::*` |
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

// Lets `#[derive(AppService)]` name `::rayx_app` from inside this crate too.
extern crate self as rayx_app;

pub mod change;
pub mod configuration;
pub mod di;
pub mod options;
pub mod published;
pub mod services;
pub mod testing;

/// What `#[contract]`'s code names in the core.
#[doc(hidden)]
pub mod __private {
    pub use futures::future::BoxFuture;
}

pub use change::ChangeSubscription;
pub use configuration::{Configuration, ConfigurationBuilder, ConfigurationError};
pub use di::{
    AppService, ServiceLifetime, ServiceProvider, ServiceResolutionError, ServiceValidationError,
    StartupRegistry,
};
pub use options::{Options, OptionsBuilder, OptionsError, OptionsMonitor, OptionsSnapshot};
pub use published::{EventStream, StateCell};
pub use services::app::{
    AppBuildError, AppBuilder, ConfigurationAppBuilderExt, RayXAppBuilder, ReadyAppBuilderExt,
    ReadyContext, ReadyError, ReadyHooks, ReadyResult, ServiceAppBuilderExt,
};

pub mod prelude {
    pub use crate::change::ChangeSubscription;
    pub use crate::configuration::{
        Configuration, ConfigurationBuilder, ConfigurationError, ConfigurationSection,
        ConfigurationSource,
    };
    pub use crate::di::{
        AppService, AsyncInitializable, DIReady, DiReady, ErasedService, IntoServiceArc,
        IntoSingletonService, LocalService, RuntimeKey, ServiceKey, ServiceLifetime,
        ServiceProvider, ServiceRegistry, ServiceResolutionError, StartupInitializable,
        StartupInitializationContext, StartupInitializationError, StartupInitializationFailure,
        StartupInitializationFailures, StartupInitializationPhase, StartupInitializationResult,
        StartupInitializationState, StartupRegistry, StartupService, StartupServiceError,
        StartupServiceFuture, StartupServiceType, default_provider,
        release_orphaned_thread_bound_services,
    };
    pub use crate::di::{ServiceValidationError, ServiceValidationProblem};
    pub use crate::options::{
        Options, OptionsBuilder, OptionsError, OptionsMonitor, OptionsSnapshot,
    };
    pub use crate::published::{EventStream, StateCell};
    pub use crate::services::app::{
        AppBuildError, AppBuilder, ConfigurationAppBuilderExt, RayXAppBuilder, ReadyAppBuilderExt,
        ReadyContext, ReadyError, ReadyHooks, ReadyResult, ServiceAppBuilderExt,
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
