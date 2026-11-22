//! RayX's core: dependency injection, configuration, options and the app builder, with the API
//! of .NET's `Microsoft.Extensions`, and every service's contract, without NSH. It builds for
//! every target, wasm32 included, and depends on no RayX crate but `rayx_app_macros`.
//!
//! - [`di`]: the [`ServiceProvider`] with singleton, scoped and transient services, scopes,
//!   async startup services, and the requirements checked before anything is made
//!   ([`ServiceProvider::require`], [`ServiceProvider::require_for`],
//!   [`ServiceProvider::validate`]).
//! - [`configuration`]: sources merged into one [`Configuration`], read by path.
//! - [`options`]: typed options made by a pipeline (bind, configure, post-configure, validate),
//!   read once, per scope or as they change.
//! - [`published`]: [`StateCell`] and [`EventStream`], the state and events a service's
//!   implementation publishes, read and followed from any thread.
//! - [`services`]: [`RayXAppBuilder`] and the [`AppBuilder`] traits service crates extend
//!   ([`services::app`]), and each service's contract (`services::<name>`: a `#[contract]`
//!   trait with its data types), which RayX's `rayx_services_<name>` crates implement on NSH
//!   and an app may implement itself, in Rust or TS.
//! - [`testing`]: a fake of each contract, for tests.
//!
//! The most used items are also re-exported at the crate root and in [`prelude`].

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
    AppService, ScopeRegistrations, ServiceLifetime, ServiceProvider, ServiceResolutionError,
    ServiceValidationError, StartupRegistry,
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
        AppService, AsyncInitializable, DIReady, DiReady, ErasedService, ImplementedBy,
        IntoServiceArc, IntoSingletonService, LocalService, RuntimeKey, ScopeRegistrations,
        ServiceKey, ServiceLifetime, ServiceProvider, ServiceRegistry, ServiceResolutionError,
        StartupInitializable, StartupInitializationContext, StartupInitializationError,
        StartupInitializationFailure, StartupInitializationFailures, StartupInitializationPhase,
        StartupInitializationResult, StartupInitializationState, StartupRegistry, StartupService,
        StartupServiceError, StartupServiceFuture, StartupServiceType, default_provider,
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
