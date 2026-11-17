//! RayX, the app framework for [NSH](https://github.com/dXnSix/nsh) apps, in one dependency: an app
//! crate depends on `rayx` and writes `use rayx::prelude::*;`.
//!
//! - [`app`] ([`rayx_app`]): the core: dependency injection, configuration, .NET's options model
//!   and the [`RayXAppBuilder`](app::RayXAppBuilder), without NSH; service crates depend on it.
//! - [`runtime`] ([`rayx_runtime`]): the app services not yet in crates of their own
//!   (configuration), without NSH.
//! - [`navigation`] ([`rayx_services_navigation`]): RayX's navigation service (routes, regions
//!   as NSH navigation stacks, the app's location as NSH's app address); its builder extension
//!   and `navigation_region`/`navigation_view` are in the prelude.
//! - [`operations`] ([`rayx_services_operations`]): RayX's operation service (targets'
//!   handlers, transactions, the operations history undoes); its builder extension is in the
//!   prelude.
//! - [`history`] ([`rayx_services_history`]): RayX's history (undo and redo per scope, in memory
//!   or in the app's key-value store); its builder extension is in the prelude.
//! - [`selection`] ([`rayx_services_selection`]): RayX's selection service (the active
//!   selection area's summary, cleared on every route change); its builder extension is in the
//!   prelude.
//! - [`app_nsh`] ([`rayx_app_nsh`]): the NSH host: a root provider per surface, a scope per screen, the
//!   app's start (`RayXAppBuilder::start`), the TSX classes (`rayx::app_nsh::ts`: `RayXAppBuilder`,
//!   `ServiceProvider` and the services' classes under their Rust names) and the services' NSH
//!   side.
//! - [`components`] ([`rayx_components`]): RayX's component library: the design system's theme
//!   (its tokens as an NSH theme document read through typed keys, its modes as the document's
//!   modifiers) and its components (`Button`), for Rust views and TSX;
//!   `rayx::components::register` adds them to an app's UI set.
//! - [`devtools_protocol`] ([`rayx_devtools_protocol`]): the agent protocol of the dev tools, the
//!   messages between an app's dev-build agent and a tool's hub, for a tool written in Rust.
//! - [`rayx_services_i18n`]'s `add_i18n`, `tr!` and [`checked::tr!`](checked::tr) in the
//!   prelude (its TS side is its set, `rayx_services_i18n::register`).
//! - [`i18n`] and [`intl`]: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
//!   [dXn.Intl](https://github.com/dXnSix/dXn.Intl), which the i18n service wraps, for their
//!   message, number and date types.
//!
//! A TSX app's crate is one attribute at its root, its UI set with every RayX TS class and
//! component ([`native_module`]); an app's own Rust services reach TS as [`app_extension`]s.
//!
//! ```no_run
//! use rayx::prelude::*;
//!
//! /// Starts the app from the root component.
//! fn start() {
//!     RayXAppBuilder::new()
//!         .add_app_settings("my-app")
//!         .add_i18n(|options| {
//!             options.default_language("en");
//!         })
//!         .start();
//! }
//! ```

pub use rayx_app as app;
pub use rayx_app_nsh as app_nsh;
pub use rayx_components as components;
pub use rayx_devtools_protocol as devtools_protocol;
pub use rayx_runtime as runtime;
pub use rayx_services_history as history;
pub use rayx_services_navigation as navigation;
pub use rayx_services_operations as operations;
pub use rayx_services_selection as selection;

pub use dxn_i18n as i18n;
pub use dxn_intl as intl;

/// Exports a Rust extension of the app builder to TS (`#[rayx::app_extension] fn
/// add_weather(builder: RayXAppBuilder) -> RayXAppBuilder` is TS's builder method
/// `builder.addWeather()`); register it in the [`native_module`] function's body
/// (`registry.class_extension::<add_weather>()`).
pub use rayx_app_nsh::app_extension;

/// Makes a Rust trait implementable from TS while Rust resolves it as `Arc<dyn Trait>`; register
/// it in the [`native_module`] function's body (`registry.contract::<dyn WeatherSource>()`).
pub use rayx_app_nsh::contract;

/// The UI set of an app's crate: NSH's `#[native_module("my-app")]` on the function, whose body
/// first registers every RayX TS class (`rayx::app_nsh::register`: the builder, the provider and its
/// tokens, the startup classes and interfaces, the services' classes) and RayX's components with
/// their theme document (`rayx::components::register`), then runs the function's own body, the
/// app's exports:
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
/// TSX imports the set by the specifier (`import { RayXAppBuilder } from "my-app"`). The crate
/// gains `pub fn registry()`, as with NSH's attribute, and NSH's CLI reads the specifier from the
/// attribute's line in the crate's source, so the set is found on every target.
#[doc(inline)]
pub use rayx_app_nsh::native_module;

/// `tr!` with the call checked against the app's messages when it builds: `use
/// rayx::checked::tr;` after the prelude's glob import uses it in place of the prelude's.
pub use rayx_services_i18n::checked;

/// What an app uses most: the core's prelude, the services' builder extensions, the NSH layer's
/// classes, hooks and builder extensions, the i18n service's builder extension, contracts and
/// arguments, and `tr!`.
pub mod prelude {
    pub use rayx_app::prelude::*;
    pub use rayx_runtime::prelude::*;

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
    pub use rayx_services_history::HistoryAppBuilderExt;
    pub use rayx_services_i18n::{Args, I18nAppBuilderExt, tr};
    pub use rayx_services_navigation::{
        NavigationAppBuilderExt, navigation_region, navigation_view,
    };
    pub use rayx_services_operations::OperationsAppBuilderExt;
    pub use rayx_services_selection::SelectionAppBuilderExt;
}
