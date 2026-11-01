//! Dependency injection: RayX's `rayx_runtime::di` without GPUI.

mod lifecycle;
mod provider;
mod ready;
mod registry;
mod service;

pub use rayx_app_macros::AppService;
pub use ready::{DIReady, DiReady};
pub use service::AppService;

pub use lifecycle::{
    AsyncInitializable, ServiceTypeList, StartupInitializable, StartupInitializationContext,
    StartupInitializationError, StartupInitializationFailure, StartupInitializationFailures,
    StartupInitializationPhase, StartupInitializationResult, StartupInitializationState,
    StartupRegistry, StartupServiceError, StartupServiceType,
};
pub use provider::{IntoServiceArc, IntoSingletonService, ServiceProvider, default_provider};
pub use registry::{ServiceLifetime, ServiceRegistry, ServiceResolutionError};
