//! RayX's dependency injection, configuration, options and app builder ([`rayx_core`]) on NSH.
//!
//! The core's lifetimes, tied to what NSH runs:
//!
//! - **Root provider: one per engine surface.** NSH forbids process-wide singletons (each
//!   window, test and hot restart is a surface of its own), so [`install`] keeps the provider a
//!   [`RayXAppBuilder`] built in the surface's `global` slot, and [`set_app`] names the builder
//!   function a surface builds its provider with when the app starts on it. Singletons live as
//!   long as their surface's provider.
//! - **Startup.** [`start_app`] (TSX: `await AppServices.load()`; a Rust root: [`start`] and
//!   [`app_state`]) builds the root with [`RayXAppBuilder::build_async`] (its configuration's
//!   sources may load asynchronously), makes the options registered with `validate_on_start`, and
//!   awaits every startup service's async initialization in dependency order, each within its
//!   timeout (its own, else the builder's default). The app is ready, and services resolve, only
//!   once all of them initialized; an invalid option, one failure or a timeout fails startup.
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
//! - **RayX's services on NSH.** Every surface's builder starts with NSH's [`NshAssetReader`]
//!   and [`NshFontRegistrar`] (reading the app's files with `fetch` of their `asset:/` URLs,
//!   registering fonts with `register_font`), which `rayx_runtime`'s asset and font services
//!   use; [`AppSettingsAppBuilderExt::add_app_settings`] adds the app's bundled
//!   `appsettings.json` as a configuration source ([`AppSettingsSource`]); and
//!   [`use_platform_sync`], in the root component, keeps the platform and suspension services
//!   current from NSH's environment and app state.
//! - **Translations.** `rayx_runtime`'s i18n service follows the platform service's languages
//!   and clock; the [`I18n`] class gives TSX its messages (`I18n.of(app).t("id", { n: 3 })`) and
//!   its changes.
//!
//! Everything here runs on the engine's thread. The services themselves stay NSH-free and depend
//! on [`rayx_core`] only; the `di`, `options`, `configuration`, `services` and `prelude` modules
//! re-export it, so `#[derive(AppService)]` works in a crate that depends on `rayx_app` alone,
//! and [`rayx_runtime`] is re-exported whole.

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod i18n_class;
mod platform;
mod scope_class;
mod screen;
mod settings;
mod surface;

pub use dxn_reactive::{ActivatableViewModel, ActivationToken, ViewModelActivator};
pub use rayx_core::{
    AppBuilder, AppService, ChangeSubscription, Configuration, ConfigurationAppBuilderExt,
    ConfigurationBuilder, ConfigurationError, Options, OptionsBuilder, OptionsError,
    OptionsMonitor, OptionsSnapshot, RayXAppBuilder, ServiceAppBuilderExt, ServiceLifetime,
    ServiceProvider, ServiceResolutionError, StartupRegistry, configuration, di, options, prelude,
    services,
};

pub use i18n_class::I18n;
pub use platform::{
    NshAssetReader, NshFontRegistrar, NshServicesAppBuilderExt, platform_snapshot,
    use_platform_sync,
};
pub use rayx_runtime;
pub use scope_class::{AppServices, ServiceScope};
pub use screen::{ScreenScope, services, use_screen_scope, use_view_model};
pub use settings::{APP_SETTINGS_FILE, AppSettingsAppBuilderExt, AppSettingsSource};
pub use surface::{
    AppError, AppState, app_services, app_state, install, set_app, start, start_app,
};
