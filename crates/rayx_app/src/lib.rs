//! RayX's dependency injection, options and app builder ([`rayx_app_di`]) on NSH.
//!
//! The core's lifetimes, tied to what NSH runs:
//!
//! - **Root provider: one per engine surface.** NSH forbids process-wide singletons (each
//!   window, test and hot restart is a surface of its own), so [`install`] keeps the provider an
//!   [`AppBuilder`] built in the surface's `global` slot, and [`set_app`] names the builder
//!   function a surface builds its provider with when the app starts on it. Singletons live as
//!   long as their surface's provider.
//! - **Startup.** [`start_app`] (TSX: `await AppServices.load()`; a Rust root: [`start`] and
//!   [`app_state`]) builds the root and awaits every startup service's async initialization in
//!   dependency order, each within its timeout (its own, else the builder's default). The app is
//!   ready, and services resolve, only once all of them initialized; one failure or timeout fails
//!   startup.
//! - **Screen scope: from mount to cleanup.** [`use_screen_scope`], in a Rust component, makes a
//!   child scope of the nearest enclosing screen's (or of the surface root), provides it to what
//!   the component renders and drops it when the component's owner cleans up, so scoped services
//!   go with their screen. TSX gets the same through the [`AppServices`] and [`ServiceScope`]
//!   classes: `app.scope()` when a screen mounts, `dispose()` in its cleanup.
//! - **Transient: per resolve**, as in the core.
//! - **Activation.** [`use_view_model`] resolves an [`ActivatableViewModel`] in the screen's scope
//!   and activates it until the component's cleanup; [`ServiceScope`] activates what it resolves
//!   for TSX until it is disposed.
//!
//! Everything here runs on the engine's thread. The services themselves stay NSH-free and depend
//! on [`rayx_app_di`] only; the `di`, `options`, `configuration`, `services` and `prelude` modules
//! re-export it, so `#[derive(AppService)]` works in a crate that depends on `rayx_app` alone.

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod scope_class;
mod screen;
mod surface;

pub use dxn_reactive::{ActivatableViewModel, ActivationToken, ViewModelActivator};
pub use rayx_app_di::{
    AppBuilder, AppService, ConfigurationAppBuilderExt, ConfigurationBuilder, ConfigurationError,
    Options, OptionsError, OptionsRegistry, ServiceAppBuilderExt, ServiceLifetime, ServiceProvider,
    ServiceResolutionError, StartupRegistry, configuration, di, options, prelude, services,
};

pub use scope_class::{AppServices, ServiceScope};
pub use screen::{ScreenScope, services, use_screen_scope, use_view_model};
pub use surface::{
    AppError, AppState, NshAppBuilder, app_services, app_state, install, set_app, start, start_app,
};
