//! RayX's dependency injection, configuration, options and app builder ([`rayx_app`]) on NSH.
//!
//! The core's lifetimes, tied to what NSH runs:
//!
//! - **Root provider: one per engine surface.** NSH forbids process-wide singletons (each
//!   window, test and hot restart is a surface of its own), so the app starts on each surface
//!   from a builder made there, and [`install`] keeps the provider it builds in the surface's
//!   `global` slot. Singletons live as long as their surface's provider.
//! - **Startup.** A Rust root builds `RayXAppBuilder::new()` with its chain and calls
//!   [`.start()`](StartAppBuilderExt::start) (watching [`app_state`]) or
//!   [`.start_async()`](StartAppBuilderExt::start_async); TSX builds [`ts::RayXAppBuilder`] and
//!   awaits its `start()`. Both build the root with [`RayXAppBuilder::build_async`] (its
//!   configuration's sources may load asynchronously), NSH's services added where the app
//!   registered none, make the options registered with `validate_on_start`, and await every
//!   startup service's async initialization in dependency order, each within its timeout (its
//!   own, else the builder's default). The app is ready, and services resolve, only once all of
//!   them initialized; an invalid option, one failure or a timeout fails startup.
//! - **Screen scope: from mount to cleanup.** [`use_screen_scope`], in a Rust component, makes a
//!   child scope of the nearest enclosing screen's (or of the surface root), provides it to what
//!   the component renders and drops it when the component's owner cleans up, so scoped services
//!   go with their screen. TSX gets the same through [`ts::ServiceProvider`]:
//!   `services.createScope()` when a screen mounts, `dispose()` in its cleanup.
//! - **Transient: per resolve**, as in the core.
//! - **Activation.** [`use_view_model`] resolves an [`ActivatableViewModel`] in the screen's scope
//!   and activates it until the component's cleanup; [`ts::ServiceProvider::view_model`]
//!   activates what a Rust bridge resolves for TSX until the provider is disposed.
//! - **Navigation regions.** [`navigation_region`] shows a region of the navigation service as
//!   an NSH navigation stack whose pages are its journal up to the current entry (a native pop
//!   takes the journal back), [`navigation_view`] as its current route alone, and
//!   [`ts::NavigationRegion`] gives TSX the same.
//! - **The location.** [`NshLocationSynchronizer`] shows the navigation's location as NSH's app
//!   address (`app_url`, `set_app_url_with`: the web's address bar and history, the link that
//!   opened a native app), takes the navigation to the address at start once the app is ready
//!   and follows the platform's moves of it (back, forward, a typed address) and the deep links
//!   that open the app while it runs.
//! - **Binding.** A view model's dXn.Reactive signals, streams, lists and commands are NSH
//!   signals in a Rust component ([`use_reactive`], [`use_stream`], [`use_two_way`],
//!   [`use_list`], [`use_command`]) and observable properties of TSX classes ([`Observable`],
//!   [`ReactiveCommand`]); a change from any thread reaches the UI on the engine's thread.
//!
//! - **RayX's services on NSH.** The service crates (`rayx_services_*`) implement RayX's
//!   services on NSH, each keeping itself current from its startup on; the app's start
//!   requires the platform, suspension, asset and font services (`add_platform()`,
//!   `add_suspension()`, `add_assets(..)`, `add_fonts(..)`, or the app's own implementations)
//!   and fails naming each one missing. Here is [`AppSettingsAppBuilderExt::add_app_settings`],
//!   which adds the app's bundled `appsettings.json` as a configuration source
//!   ([`AppSettingsSource`]).
//! - **Translations.** `rayx_runtime`'s i18n service follows the platform service's languages
//!   and clock; [`tr!`] gives a Rust component its messages (`text(move || tr!("id", n = 3))`,
//!   following the language; [`checked::tr!`](checked::tr) checks the call against the app's
//!   messages when it builds), and the [`ts::I18nService`] class gives TSX them
//!   (`services.getRequiredService(I18nService).t("id", { n: 3 })`) and their changes.
//! - **The services for TSX** ([`ts`]): each RayX service's TS class has the Rust service's name
//!   and is its own token (`I18nService`, `NavigationService` and
//!   `ApplicationConfigurationService` here; `AssetService`, `FontService`, `KeyValueStore`,
//!   `PlatformInfoService` and `SuspensionService` in their crates' sets), and its `onChange`
//!   hears the service's changes.
//! - **Services written in TS** live in the same container: `addSingleton(Clock, () => new
//!   Clock())` and every other lifetime, startup and `As` form, keyed by a TS class or a
//!   [`ts::ServiceToken`], with the TS startup interfaces `AsyncInitializable` and
//!   `StartupInitializable` in the same startup graph, and TS options (`addOptions`,
//!   `configureOptions`, `getOptions`). They are made and resolved on the engine's thread only.
//!   Rust calls a TS service only through a contract: a trait marked `#[contract]`, resolved as
//!   `Arc<dyn Trait>` ([`ts::ContractError`] off the engine thread).
//! - **Extensions.** `#[app_extension]` exports a Rust builder extension to TS as a builder
//!   method (`add_weather` is `builder.addWeather()`), and a service crate adds its TS builder
//!   method the same way, a class extension of [`ts::RayXAppBuilder`] acting on it through
//!   [`ts::RayXAppBuilder::extend_with`] and [`ts::record`]; `#[rayx::native_module("spec")]`,
//!   in the umbrella, makes an app's UI set with [`ts::register`]'s classes and RayX's
//!   components.
//!
//! Everything here runs on the engine's thread. The services themselves stay NSH-free and depend
//! on [`rayx_app`] only; the `di`, `options`, `configuration`, `services` and `prelude` modules
//! re-export it, so `#[derive(AppService)]` works in a crate that depends on `rayx_app_nsh` alone,
//! and [`rayx_runtime`] is re-exported whole.

// Unit tests panic on broken fixtures by design.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

// The code `#[app_extension]` and `#[contract]` generate names this crate `::rayx_app_nsh`, in its
// own tests too.
extern crate self as rayx_app_nsh;

mod app;
mod binding;
pub mod configuration;
mod devtools;
pub mod di;
pub mod services;
pub mod ts;

/// The headless app harness RayX's and service crates' tests use (feature `testing`): a TSX app
/// on NSH's engine with a UI set of RayX's TS classes and the test's own ([`testing::set`]), its
/// declarations ([`testing::declarations`]) and the text it shows ([`testing::run`]).
#[cfg(feature = "testing")]
pub mod testing;

#[doc(hidden)]
pub use rayx_app_macros::__contract_nsh_expand;
/// The umbrella's `#[rayx::native_module("spec")]`, which names `rayx`: use it through `rayx`.
#[doc(hidden)]
pub use rayx_app_macros::native_module;
pub use rayx_app_macros::{app_extension, contract, contract_nsh};
pub use ts::register;

pub use dxn_reactive::{ActivatableViewModel, ActivationToken, ViewModelActivator};
pub use rayx_app::{
    AppBuilder, AppService, ChangeSubscription, Configuration, ConfigurationAppBuilderExt,
    ConfigurationBuilder, ConfigurationError, EventStream, Options, OptionsBuilder, OptionsError,
    OptionsMonitor, OptionsSnapshot, RayXAppBuilder, ReadyAppBuilderExt, ReadyContext, ReadyError,
    ReadyHooks, ReadyResult, ServiceAppBuilderExt, ServiceLifetime, ServiceProvider,
    ServiceResolutionError, StartupRegistry, StateCell, options, prelude,
};

pub use app::surface::{AppError, AppState, StartAppBuilderExt, app_services, app_state, install};
pub use binding::{
    EngineGone, EngineRunner, Observable, ReactiveCommand, UiCommand, use_command,
    use_event_stream, use_list, use_reactive, use_state_cell, use_stream, use_two_way,
};
pub use configuration::settings::{APP_SETTINGS_FILE, AppSettingsAppBuilderExt, AppSettingsSource};
pub use devtools::{
    DevLayer, DevTools, DevToolsOptions, dev_layer, devtools_backoff, devtools_enabled,
    devtools_hook_calls, start_devtools,
};
pub use di::screen::{ScreenScope, services, use_screen_scope, use_view_model};
pub use rayx_runtime;
#[doc(hidden)]
pub use services::i18n::translate::__private;
pub use services::i18n::translate::{checked, translate};
pub use services::navigation::location::{NshLocationSynchronizer, location_of_address};
pub use services::navigation::region::{navigation_region, navigation_view};
