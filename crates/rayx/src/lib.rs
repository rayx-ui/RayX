//! RayX, the app framework for [NSH](https://github.com/dXnSix/nsh) apps, in one dependency: an app
//! crate depends on `rayx` and writes `use rayx::prelude::*;`.
//!
//! - [`app`] ([`rayx_app`]): the core: dependency injection, configuration, .NET's options model,
//!   the [`RayXAppBuilder`](app::RayXAppBuilder) and every service's contract
//!   (`rayx::app::services::<name>`, the trait an app resolves and may implement itself), without
//!   NSH.
//! - [`app_nsh`] ([`rayx_app_nsh`]): the NSH host: a root provider per surface, a scope per
//!   screen, the app's start (`RayXAppBuilder::start`) and the TSX classes (`rayx::app_nsh::ts`:
//!   `RayXAppBuilder`, `ServiceProvider`, the startup and options classes).
//! - [`services`]: RayX's implementations of the services, one crate and one feature each, an
//!   enabled crate at `rayx::services::<name>` with its builder extension in the prelude.
//! - [`components`] ([`rayx_components`]): RayX's component library: the design system's roles
//!   (`theme::roles`), its default theme (an NSH theme document that maps them, whose modes are
//!   its modifiers) and its components (`Button`), for Rust views and TSX.
//! - [`devtools_protocol`] ([`rayx_devtools_protocol`]): the agent protocol of the dev tools, the
//!   messages between an app's dev-build agent and a tool's hub, for a tool written in Rust.
//! - [`i18n`] and [`intl`]: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
//!   [dXn.Intl](https://github.com/dXnSix/dXn.Intl), which the i18n service wraps, for their
//!   message, number and date types.
//!
//! # Features
//!
//! One per service crate, named after the service, each enabling that crate alone; none implies
//! another (a service that needs one the app did not register fails the app's start, naming it):
//!
//! | Feature | Crate, at `rayx::services::<name>` | Builder extension in the prelude | Default |
//! | --- | --- | --- | --- |
//! | `platform` | `rayx_services_platform` | `PlatformAppBuilderExt` (`add_platform`) | yes |
//! | `suspension` | `rayx_services_suspension` | `SuspensionAppBuilderExt` (`add_suspension`) | yes |
//! | `assets` | `rayx_services_assets` | `AssetAppBuilderExt` (`add_assets`) | yes |
//! | `fonts` | `rayx_services_fonts` | `FontAppBuilderExt` (`add_fonts`) | yes |
//! | `application_configuration` | `rayx_services_application_configuration` | `ApplicationConfigurationAppBuilderExt` | |
//! | `history` | `rayx_services_history` | `HistoryAppBuilderExt` | |
//! | `i18n` | `rayx_services_i18n`, with `tr!` in the prelude and `rayx::checked` | `I18nAppBuilderExt` | |
//! | `input_map` | `rayx_services_input_map` | `InputMapAppBuilderExt` | |
//! | `navigation` | `rayx_services_navigation`, with `navigation_region`/`navigation_view` in the prelude | `NavigationAppBuilderExt` | |
//! | `operations` | `rayx_services_operations` | `OperationsAppBuilderExt` | |
//! | `selection` | `rayx_services_selection` | `SelectionAppBuilderExt` | |
//! | `storage` | `rayx_services_storage` | `StorageAppBuilderExt` | |
//!
//! The defaults are the four services the host requires; a feature off removes its crate from
//! the build. `rayx = { version = "…", features = ["i18n", "storage"] }` adds two.
//!
//! A TSX app's crate is one attribute at its root, its UI set with RayX's TS classes, every
//! enabled service's set and RayX's components ([`native_module`]); an app's own Rust services
//! reach TS as [`app_extension`]s.
//!
//! ```no_run
//! # #[cfg(all(feature = "platform", feature = "suspension", feature = "assets", feature = "fonts"))]
//! # mod app {
//! use rayx::prelude::*;
//!
//! /// Starts the app from the root component.
//! fn start() {
//!     RayXAppBuilder::new()
//!         .add_platform()
//!         .add_suspension()
//!         .add_assets(|_| {})
//!         .add_fonts(|_| {})
//!         .add_app_settings("my-app")
//!         .start();
//! }
//! # }
//! ```

pub use rayx_app as app;
pub use rayx_app_nsh as app_nsh;
pub use rayx_components as components;
pub use rayx_devtools_protocol as devtools_protocol;

pub use dxn_i18n as i18n;
pub use dxn_intl as intl;

/// RayX's implementations of the services, one crate per service, each behind the feature of its
/// name: an enabled crate is `rayx::services::<name>`, its builder extension is in the
/// [`prelude`], and its TS side is in every [`native_module`]'s set. The traits they implement
/// are the core's, `rayx::app::services::<name>`.
pub mod services {
    #[cfg(feature = "application_configuration")]
    pub use rayx_services_application_configuration as application_configuration;
    #[cfg(feature = "assets")]
    pub use rayx_services_assets as assets;
    #[cfg(feature = "fonts")]
    pub use rayx_services_fonts as fonts;
    #[cfg(feature = "history")]
    pub use rayx_services_history as history;
    #[cfg(feature = "i18n")]
    pub use rayx_services_i18n as i18n;
    #[cfg(feature = "input_map")]
    pub use rayx_services_input_map as input_map;
    #[cfg(feature = "navigation")]
    pub use rayx_services_navigation as navigation;
    #[cfg(feature = "operations")]
    pub use rayx_services_operations as operations;
    #[cfg(feature = "platform")]
    pub use rayx_services_platform as platform;
    #[cfg(feature = "selection")]
    pub use rayx_services_selection as selection;
    #[cfg(feature = "storage")]
    pub use rayx_services_storage as storage;
    #[cfg(feature = "suspension")]
    pub use rayx_services_suspension as suspension;
}

/// Exports a Rust extension of the app builder to TS (`#[rayx::app_extension] fn
/// add_weather(builder: RayXAppBuilder) -> RayXAppBuilder` is TS's builder method
/// `builder.addWeather()`); register it in the [`native_module`] function's body
/// (`registry.class_extension::<add_weather>()`).
pub use rayx_app_nsh::app_extension;

/// Makes a Rust trait implementable from TS while Rust resolves it as `Arc<dyn Trait>`; register
/// it in the [`native_module`] function's body (`registry.contract::<dyn WeatherSource>()`).
pub use rayx_app_nsh::contract;

/// The UI set of an app's crate: NSH's `#[native_module("my-app")]` on the function, whose body
/// first registers RayX's TS classes (`rayx::app_nsh::register`: the builder, the provider and
/// its tokens, the startup and options classes), the set of every service crate the umbrella's
/// features enable (its classes, its contract's interface and its builder method:
/// `addFonts`, `addI18n`, …) and RayX's components with their theme document
/// (`rayx::components::register`), then runs the function's own body, the app's exports:
///
/// ```ignore
/// use rayx::prelude::*;
///
/// #[rayx::native_module("my-app")]
/// fn native(registry: &mut Registry) {}
///
/// #[rayx::native_module("my-app")]
/// fn native(registry: &mut Registry) {
///     registry.register::<Glyph>().class_extension::<add_weather>();
/// }
/// ```
///
/// With the default features a TS `RayXAppBuilder` has `addPlatform`, `addSuspension`,
/// `addAssets` and `addFonts`; `features = ["i18n"]` adds `addI18n` and the `I18nService` class.
/// TSX imports the set by the specifier (`import { RayXAppBuilder } from "my-app"`). The crate
/// gains `pub fn registry()`, as with NSH's attribute, and NSH's CLI reads the specifier from the
/// attribute's line in the crate's source, so the set is found on every target.
#[doc(inline)]
pub use rayx_app_nsh::native_module;

/// `tr!` with the call checked against the app's messages when it builds: `use
/// rayx::checked::tr;` after the prelude's glob import uses it in place of the prelude's
/// (feature `i18n`).
#[cfg(feature = "i18n")]
pub use rayx_services_i18n::checked;

/// What [`native_module`] expands to; not part of the API.
#[doc(hidden)]
pub mod __private {
    use rayx_app_nsh::__private::nsh_core::element::Registry;

    /// Registers RayX's TS classes, the set of each service crate the umbrella's features enable
    /// and RayX's components, in that order: what an app's set holds before its own exports.
    pub fn register(registry: &mut Registry) {
        rayx_app_nsh::register(registry);
        #[cfg(feature = "application_configuration")]
        rayx_services_application_configuration::register(registry);
        #[cfg(feature = "assets")]
        rayx_services_assets::register(registry);
        #[cfg(feature = "fonts")]
        rayx_services_fonts::register(registry);
        #[cfg(feature = "history")]
        rayx_services_history::register(registry);
        #[cfg(feature = "i18n")]
        rayx_services_i18n::register(registry);
        #[cfg(feature = "input_map")]
        rayx_services_input_map::register(registry);
        #[cfg(feature = "navigation")]
        rayx_services_navigation::register(registry);
        #[cfg(feature = "operations")]
        rayx_services_operations::register(registry);
        #[cfg(feature = "platform")]
        rayx_services_platform::register(registry);
        #[cfg(feature = "selection")]
        rayx_services_selection::register(registry);
        #[cfg(feature = "storage")]
        rayx_services_storage::register(registry);
        #[cfg(feature = "suspension")]
        rayx_services_suspension::register(registry);
        rayx_components::register(registry);
    }
}

/// What an app uses most: the core's prelude, the NSH layer's classes, hooks and builder
/// extensions, and each enabled service crate's builder extension (with `tr!` and the i18n
/// contracts for `i18n`, `navigation_region` and `navigation_view` for `navigation`).
pub mod prelude {
    pub use rayx_app::prelude::*;

    #[cfg(feature = "i18n")]
    pub use rayx_app::services::i18n::{I18nService, MessageFormatter};
    /// The registry a [`native_module`](crate::native_module) function adds the app's exports to.
    pub use rayx_app_nsh::__private::nsh_core::element::Registry;
    pub use rayx_app_nsh::ts::ContractRegistryExt;
    pub use rayx_app_nsh::{ActivatableViewModel, ActivationToken, ViewModelActivator};
    pub use rayx_app_nsh::{
        AppError, AppSettingsAppBuilderExt, AppState, Observable, ReactiveCommand, ScreenScope,
        StartAppBuilderExt, UiCommand, app_services, app_state, install, services, use_command,
        use_event_stream, use_list, use_reactive, use_screen_scope, use_state_cell, use_stream,
        use_two_way, use_view_model,
    };
    pub use rayx_app_nsh::{DevLayer, DevTools, dev_layer};
    #[cfg(feature = "application_configuration")]
    pub use rayx_services_application_configuration::ApplicationConfigurationAppBuilderExt;
    #[cfg(feature = "assets")]
    pub use rayx_services_assets::AssetAppBuilderExt;
    #[cfg(feature = "fonts")]
    pub use rayx_services_fonts::FontAppBuilderExt;
    #[cfg(feature = "history")]
    pub use rayx_services_history::HistoryAppBuilderExt;
    #[cfg(feature = "i18n")]
    pub use rayx_services_i18n::{Args, I18nAppBuilderExt, tr};
    #[cfg(feature = "input_map")]
    pub use rayx_services_input_map::InputMapAppBuilderExt;
    #[cfg(feature = "navigation")]
    pub use rayx_services_navigation::{
        NavigationAppBuilderExt, navigation_region, navigation_view,
    };
    #[cfg(feature = "operations")]
    pub use rayx_services_operations::OperationsAppBuilderExt;
    #[cfg(feature = "platform")]
    pub use rayx_services_platform::PlatformAppBuilderExt;
    #[cfg(feature = "selection")]
    pub use rayx_services_selection::SelectionAppBuilderExt;
    #[cfg(feature = "storage")]
    pub use rayx_services_storage::StorageAppBuilderExt;
    #[cfg(feature = "suspension")]
    pub use rayx_services_suspension::SuspensionAppBuilderExt;
}
