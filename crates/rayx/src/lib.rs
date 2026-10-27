//! RayX, the app framework for [NSH](https://github.com/dXnSix/nsh) apps, in one dependency: an app
//! crate depends on `rayx` and writes `use rayx::prelude::*;`.
//!
//! - [`core`] ([`rayx_core`]): dependency injection, configuration, .NET's options model and the
//!   [`RayXAppBuilder`](core::RayXAppBuilder), without NSH; service crates depend on it alone.
//! - [`runtime`] ([`rayx_runtime`]): the app services (storage, operations and history,
//!   navigation, input maps, platform and lifecycle state, configuration, assets, fonts and
//!   translations), without NSH.
//! - [`app`] ([`rayx_app`]): the NSH layer: a root provider per surface, a scope per screen, the
//!   TSX classes (`AppServices`, `ServiceScope`, `I18n`) and the services' NSH side.
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
        AppError, AppServices, AppSettingsAppBuilderExt, AppState, I18n, NshServicesAppBuilderExt,
        ScreenScope, ServiceScope, app_services, app_state, install, set_app, start, start_app,
        use_platform_sync, use_screen_scope, use_view_model,
    };
    pub use rayx_runtime::services::i18n::{Args, I18nService};
}
