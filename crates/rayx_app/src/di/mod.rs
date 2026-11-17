//! Dependency injection: RayX's `rayx_runtime::di` without GPUI.

mod key;
mod lifecycle;
mod provider;
mod ready;
mod registry;
mod service;
mod thread_bound;
mod validation;

pub use key::{ErasedService, RuntimeKey, ServiceKey};
pub use rayx_app_macros::AppService;
pub use ready::{DIReady, DiReady};
pub use service::AppService;
pub use thread_bound::{LocalService, ThreadBound, release_orphaned_thread_bound_services};

pub use lifecycle::{
    AsyncInitializable, ServiceTypeList, StartupInitializable, StartupInitializationContext,
    StartupInitializationError, StartupInitializationFailure, StartupInitializationFailures,
    StartupInitializationPhase, StartupInitializationResult, StartupInitializationState,
    StartupRegistry, StartupService, StartupServiceError, StartupServiceFuture, StartupServiceType,
};
pub use provider::{
    IntoServiceArc, IntoSingletonService, ServiceProvider, WeakServiceProvider, default_provider,
};
pub use registry::{ServiceLifetime, ServiceRegistry, ServiceResolutionError};
pub use validation::{ServiceValidationError, ServiceValidationProblem};
