//! RayX for TSX: the classes a TSX app builds, starts and resolves its services with, named as
//! their Rust counterparts.
//!
//! ```ts
//! const services = await new RayXAppBuilder()
//!   .addStorage()
//!   .addI18n((options) => { options.defaultLanguage("en"); })
//!   .start();
//! const i18n = services.getRequiredService(I18nService);
//! ```
//!
//! - [`RayXAppBuilder`]: the composition root, a camelCase twin of every Rust builder method
//!   (`addI18n(options => …)` with the options classes of [`options`](self#options-classes)),
//!   and `await builder.start()`.
//! - [`ServiceProvider`]: what `start()` resolves with: `getService`, `getRequiredService`,
//!   `createScope` and `dispose`. A RayX service's class is its own token, and a
//!   [`ServiceToken`] keys a service with no Rust type; the declarations type both lookups over
//!   `ServiceKey<T>`, so `getRequiredService(I18nService)` is an `I18nService` in TS.
//! - The services' classes: [`I18nService`], [`NavigationService`] (with
//!   [`NavigationRegion`]s), [`KeyValueStore`], [`ApplicationConfigurationService`],
//!   [`AssetService`], [`FontService`], [`PlatformInfoService`] and [`SuspensionService`].
//!
//! # Options classes
//!
//! [`I18nOptions`], [`NavigationOptions`], [`AssetOptions`], [`FontOptions`] (with
//! [`FontAliasOptions`]), [`InputMapOptions`], [`ApplicationConfigurationOptions`] and the [`ConfigurationBuilder`] of
//! `configureAppConfiguration`: what a builder method's callback gets, valid during the call.
//!
//! The structs live here, not at the crate root, so their names (`I18nService`) are the Rust
//! services' without clashing with [`rayx_runtime`]'s types. [`register`] adds them all to a UI
//! set.

mod app_options;
mod builder;
mod container;
mod contract;
mod i18n;
mod listeners;
mod options;
mod provider;
mod ready;
mod region;
mod services;
mod startup;

use nsh_core::element::{Registry, TsAlias};

pub use app_options::{OptionsBuilder, OptionsToken};
pub use builder::RayXAppBuilder;
#[doc(hidden)]
pub use contract::__private;
pub use contract::{Contract, ContractError, ContractRegistryExt, Exception, contract_token};
pub use i18n::{DateFormat, DateLength, I18nService, NumberFormat, NumberKind, TimeDetail};
pub use options::{
    ApplicationConfigurationOptions, AssetOptions, ConfigurationBuilder, FontAliasOptions,
    FontOptions, I18nOptions, InputMapOptions, NavigationOptions, RouteOptions,
};
pub use provider::{Resolved, ServiceKeyArg, ServiceProvider, ServiceToken};
pub use ready::ReadyContext;
pub use region::NavigationRegion;
pub use services::{
    ApplicationConfigurationService, AssetService, FontService, KeyValueStore, NavigationService,
    PlatformInfoService, SuspensionService,
};
pub use startup::{
    AsyncInitializable, JsAsyncInitializable, JsStartupInitializable, StartupInitializable,
    StartupInitializationContext, StartupInitializationState,
};

/// What names a service of type `T` in TS: its class (a RayX service's, whose constructor is
/// private, so it is taken through its `prototype`), or a [`ServiceToken`].
pub const SERVICE_KEY: TsAlias = TsAlias {
    name: "ServiceKey<T>",
    ts: "{ readonly prototype: T } | ServiceToken<T>",
    doc: "What names a service of type `T`: its class (`I18nService`), or a `ServiceToken<T>`.",
};

/// Adds RayX's TS classes to `registry` (an app's UI set): the builder, the provider and its
/// tokens (with the `ServiceKey<T>` alias), the TS options' `OptionsToken` and
/// `OptionsBuilder`, the startup interfaces (`AsyncInitializable`,
/// `StartupInitializable`) and classes, the options classes and the services' classes.
pub fn register(registry: &mut Registry) {
    crate::devtools::register();
    registry
        .ts_alias(SERVICE_KEY)
        .class::<RayXAppBuilder>()
        .class::<ServiceProvider>()
        .class::<ServiceToken>()
        .interface::<JsAsyncInitializable>()
        .interface::<JsStartupInitializable>()
        .class::<StartupInitializationState>()
        .class::<StartupInitializationContext>()
        .class::<ReadyContext>()
        .class::<OptionsToken>()
        .class::<OptionsBuilder>()
        .class::<ConfigurationBuilder>()
        .class::<ApplicationConfigurationOptions>()
        .class::<I18nOptions>()
        .class::<NavigationOptions>()
        .class::<AssetOptions>()
        .class::<FontOptions>()
        .class::<FontAliasOptions>()
        .class::<InputMapOptions>()
        .class::<I18nService>()
        .class::<NavigationService>()
        .class::<NavigationRegion>()
        .class::<KeyValueStore>()
        .class::<ApplicationConfigurationService>()
        .class::<AssetService>()
        .class::<FontService>()
        .class::<PlatformInfoService>()
        .class::<SuspensionService>();
}
