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
//! The structs live beside their services (`services/<name>/`, `di/`, `app/`, `configuration/`)
//! and are re-exported here, not at the crate root, so their names (`I18nService`) are the Rust
//! services' without clashing with [`rayx_runtime`]'s types. [`register`] adds them all to a UI
//! set.

use nsh_core::element::{Registry, TsAlias};

pub use crate::app::builder_ts::RayXAppBuilder;
pub use crate::app::ready_ts::ReadyContext;
pub use crate::configuration::configuration_builder_ts::ConfigurationBuilder;
pub use crate::configuration::options_ts::{OptionsBuilder, OptionsToken};
#[doc(hidden)]
pub use crate::di::contract_ts::__private;
pub use crate::di::contract_ts::{
    Contract, ContractError, ContractEvents, ContractRegistryExt, ContractState, Exception,
    contract_token,
};
pub use crate::di::provider_ts::{
    Resolved, ServiceClass, ServiceKeyArg, ServiceProvider, ServiceToken,
};
pub use crate::di::startup_ts::{
    AsyncInitializable, JsAsyncInitializable, JsStartupInitializable, StartupInitializable,
    StartupInitializationContext, StartupInitializationState,
};
pub use crate::services::application_configuration::application_configuration_ts::ApplicationConfigurationService;
pub use crate::services::application_configuration::options_ts::ApplicationConfigurationOptions;
pub use crate::services::assets::assets_ts::AssetService;
pub use crate::services::assets::options_ts::AssetOptions;
pub use crate::services::fonts::fonts_ts::FontService;
pub use crate::services::fonts::options_ts::{FontAliasOptions, FontOptions};
pub use crate::services::i18n::i18n_ts::{
    DateFormat, DateLength, I18nService, NumberFormat, NumberKind, TimeDetail,
};
pub use crate::services::i18n::options_ts::I18nOptions;
pub use crate::services::input_map::options_ts::InputMapOptions;
pub use crate::services::navigation::navigation_ts::NavigationService;
pub use crate::services::navigation::options_ts::{NavigationOptions, RouteOptions};
pub use crate::services::navigation::region_ts::NavigationRegion;
pub use crate::services::platform::platform_ts::PlatformInfoService;
pub use crate::services::storage::storage_ts::KeyValueStore;
pub use crate::services::suspension::suspension_ts::SuspensionService;

/// What names a service of type `T` in TS: its class (a RayX service's, whose constructor is
/// private, so it is taken through its `prototype`), or a [`ServiceToken`].
pub const SERVICE_KEY: TsAlias = TsAlias {
    name: "ServiceKey<T>",
    ts: "{ readonly prototype: T } | ServiceToken<T>",
    doc: "What names a service of type `T`: its class (`I18nService`), or a `ServiceToken<T>`.",
};

/// A contract's class ([`ContractRegistryExt::contract_class`]), by the interface `T` its
/// registrations implement: what the builder's registrations take before [`SERVICE_KEY`], so
/// their factories are typed by the interface.
pub const CONTRACT_CLASS: TsAlias = TsAlias {
    name: "ContractClass<T>",
    ts: "{ readonly prototype: { readonly contract: T | null } }",
    doc: "A contract's class, which TS registers implementations of the contract `T` under.",
};

/// Adds RayX's TS classes to `registry` (an app's UI set): the builder, the provider and its
/// tokens (with the `ServiceKey<T>` alias), the TS options' `OptionsToken` and
/// `OptionsBuilder`, the startup interfaces (`AsyncInitializable`,
/// `StartupInitializable`) and classes, the options classes and the services' classes.
pub fn register(registry: &mut Registry) {
    crate::devtools::register();
    registry
        .ts_alias(SERVICE_KEY)
        .ts_alias(CONTRACT_CLASS)
        .class::<RayXAppBuilder>()
        .class::<ServiceProvider>()
        .class::<ServiceToken>()
        .class::<ContractState>()
        .class::<ContractEvents>()
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
