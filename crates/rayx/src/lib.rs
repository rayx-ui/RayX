//! RayX, the app framework for [NSH](https://github.com/dXnSix/nsh) apps, in one dependency: an app
//! crate depends on `rayx` and writes `use rayx::prelude::*;`.
//!
//! - [`core`] ([`rayx_core`]): dependency injection, configuration, .NET's options model and the
//!   [`RayXAppBuilder`](core::RayXAppBuilder), without NSH; service crates depend on it alone.
//! - [`runtime`] ([`rayx_runtime`]): the app services (storage, operations and history,
//!   navigation, input maps, platform and lifecycle state, configuration, assets, fonts and
//!   translations), without NSH.
//! - [`app`] ([`rayx_app`]): the NSH layer: a root provider per surface, a scope per screen, the
//!   TSX classes (`AppServices`, `ServiceScope`, `I18n`, `Navigation`, `Storage`,
//!   `AppConfiguration`, `Assets`, `Fonts`, `Platform`) and the services' NSH side.
//! - [`i18n`] and [`intl`]: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
//!   [dXn.Intl](https://github.com/dXnSix/dXn.Intl), which the i18n service wraps, for their
//!   message, number and date types.
//!
//! ```no_run
//! use rayx::prelude::*;
//!
//! fn build(builder: RayXAppBuilder) -> RayXAppBuilder {
//!     builder
//!         .add_app_settings("my-app")
//!         .add_storage()
//!         .add_i18n(|options| {
//!             options.default_language("en");
//!         })
//! }
//!
//! set_app(build);
//! ```

pub use rayx_app as app;
pub use rayx_core as core;
pub use rayx_runtime as runtime;

pub use dxn_i18n as i18n;
pub use dxn_intl as intl;

/// What an app uses most: the core's prelude, the services' builder extensions, the NSH layer's
/// classes, hooks and builder extensions, and the i18n arguments.
pub mod prelude {
    pub use rayx_core::prelude::*;
    pub use rayx_runtime::prelude::*;

    pub use rayx_app::{ActivatableViewModel, ActivationToken, ViewModelActivator};
    pub use rayx_app::{
        AppConfiguration, AppError, AppServices, AppSettingsAppBuilderExt, AppState, Assets, Fonts,
        I18n, Navigation, NavigationRegion, NshServicesAppBuilderExt, Observable, Platform,
        ReactiveCommand, ScreenScope, ServiceScope, Storage, UiCommand, app_services, app_state,
        install, navigation_region, navigation_view, services, set_app, start, start_app,
        use_command, use_list, use_platform_sync, use_reactive, use_screen_scope, use_stream,
        use_two_way, use_view_model,
    };
    pub use rayx_runtime::services::i18n::{Args, I18nService};
}
