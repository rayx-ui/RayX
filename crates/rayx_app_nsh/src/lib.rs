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
//! - **RayX's services on NSH.** Every surface's builder starts with NSH's [`NshAssetReader`]
//!   and [`NshFontRegistrar`] (reading the app's files with `fetch` of their `asset:/` URLs,
//!   registering fonts with `register_font`), which `rayx_runtime`'s asset and font services
//!   use, [`NshKeyValueStore`], the app's persistent key-value store in NSH's storage, and
//!   [`NshSuspensionHost`], which quits the app and asks before quitting for the suspension
//!   service; [`AppSettingsAppBuilderExt::add_app_settings`] adds the app's bundled
//!   `appsettings.json` as a configuration source ([`AppSettingsSource`]); and
//!   [`use_platform_sync`], in the root component, keeps the platform and suspension services
//!   current from NSH's environment and app state, the system's wakes and the app's quitting.
//! - **Translations.** `rayx_runtime`'s i18n service follows the platform service's languages
//!   and clock; [`tr!`] gives a Rust component its messages (`text(move || tr!("id", n = 3))`,
//!   following the language; [`checked::tr!`](checked::tr) checks the call against the app's
//!   messages when it builds), and the [`ts::I18nService`] class gives TSX them
//!   (`services.getRequiredService(I18nService).t("id", { n: 3 })`) and their changes.
//! - **The services for TSX** ([`ts`]): each RayX service's TS class has the Rust service's name
//!   and is its own token (`I18nService`, `NavigationService`, `KeyValueStore`,
//!   `ApplicationConfigurationService`, `AssetService`, `FontService`, `PlatformInfoService`,
//!   `SuspensionService`), and its `onChange` hears the service's changes. A TSX app has no root
//!   component, so its `builder.start()` keeps the platform and suspension services current
//!   itself, as [`use_platform_sync`] does in a Rust root.
//! - **Services written in TS** live in the same container: `addSingleton(Clock, () => new
//!   Clock())` and every other lifetime, startup and `As` form, keyed by a TS class or a
//!   [`ts::ServiceToken`], with the TS startup interfaces `AsyncInitializable` and
//!   `StartupInitializable` in the same startup graph, and TS options (`addOptions`,
//!   `configureOptions`, `getOptions`). They are made and resolved on the engine's thread only.
//!   Rust calls a TS service only through a contract: a trait marked `#[contract]`, resolved as
//!   `Arc<dyn Trait>` ([`ts::ContractError`] off the engine thread).
//! - **Extensions.** `#[app_extension]` exports a Rust builder extension to TS (`add_weather` is
//!   `addWeather`, applied with `builder.apply(addWeather)`); `#[rayx::native_module("spec")]`,
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

mod binding;
mod devtools;
mod i18n_locale;
mod location;
mod platform;
mod region;
mod screen;
mod settings;
mod storage;
mod surface;
mod suspension;
mod translate;
pub mod ts;

/// The umbrella's `#[rayx::native_module("spec")]`, which names `rayx`: use it through `rayx`.
#[doc(hidden)]
pub use rayx_app_macros::native_module;
pub use rayx_app_macros::{app_extension, contract};
pub use ts::register;

pub use dxn_reactive::{ActivatableViewModel, ActivationToken, ViewModelActivator};
pub use rayx_app::{
    AppBuilder, AppService, ChangeSubscription, Configuration, ConfigurationAppBuilderExt,
    ConfigurationBuilder, ConfigurationError, Options, OptionsBuilder, OptionsError,
    OptionsMonitor, OptionsSnapshot, RayXAppBuilder, ReadyAppBuilderExt, ReadyContext, ReadyError,
    ReadyHooks, ReadyResult, ServiceAppBuilderExt, ServiceLifetime, ServiceProvider,
    ServiceResolutionError, StartupRegistry, configuration, di, options, prelude, services,
};

pub use binding::{
    Observable, ReactiveCommand, UiCommand, use_command, use_list, use_reactive, use_stream,
    use_two_way,
};
pub use devtools::{
    DevLayer, DevTools, DevToolsOptions, dev_layer, devtools_backoff, devtools_enabled,
    devtools_hook_calls, start_devtools,
};
pub use location::{NshLocationSynchronizer, location_of_address};
pub use platform::{
    NshAssetReader, NshFontRegistrar, NshServicesAppBuilderExt, platform_snapshot,
    use_platform_sync,
};
pub use rayx_runtime;
pub use region::{navigation_region, navigation_view};
pub use screen::{ScreenScope, services, use_screen_scope, use_view_model};
pub use settings::{APP_SETTINGS_FILE, AppSettingsAppBuilderExt, AppSettingsSource};
pub use storage::{NshKeyValueStore, STORAGE_PREFIX};
pub use surface::{AppError, AppState, StartAppBuilderExt, app_services, app_state, install};
pub use suspension::NshSuspensionHost;
#[doc(hidden)]
pub use translate::__private;
pub use translate::{checked, translate};
