//! RayX for TSX: the classes a TSX app builds, starts and resolves its services with, named as
//! their Rust counterparts.
//!
//! ```ts
//! const services = await new RayXAppBuilder()
//!   .addNavigation((options) => options.addRoute("/"))
//!   .start();
//! const navigation = services.getRequiredService(NavigationService);
//! ```
//!
//! - [`RayXAppBuilder`]: the composition root, a camelCase twin of every Rust builder method
//!   (`addNavigation(options => …)` with the options classes of [`options`](self#options-classes)),
//!   and `await builder.start()`.
//! - [`ServiceProvider`]: what `start()` resolves with: `getService`, `getRequiredService`,
//!   `createScope` and `dispose`. A RayX service's class is its own token, and a
//!   [`ServiceToken`] keys a service with no Rust type; the declarations type both lookups over
//!   `ServiceKey<T>`, so `getRequiredService(I18nService)` is an `I18nService` in TS.
//! - The services' classes: the service crates (`rayx_services_*`) add theirs
//!   (`ApplicationConfigurationService`, `AssetService`, `FontService`, `HistoryService`,
//!   `I18nService`, `InputMapService`, `KeyValueStore`, `NavigationService` with
//!   `NavigationRegion`, `OperationService` with `OperationTarget`, `PlatformInfoService`,
//!   `SelectionService`, `SuspensionService`) with their sets, through [`ServiceClass`] and
//!   [`ContractRegistryExt`].
//!
//! # Options classes
//!
//! The [`ConfigurationBuilder`] of `configureAppConfiguration`: what a builder method's callback
//! gets, valid during the call (a service crate's options classes, `I18nOptions` and the others,
//! come with its set).
//!
//! The structs live beside what they wrap (`di/`, `app/`, `configuration/`, `services/`) and are
//! re-exported here, not at the crate root, so their names do not clash with the services'
//! types. [`register`] adds them all to a UI set.

use nsh_core::element::{Registry, TsAlias};

pub use crate::app::builder_ts::{ContractFactory, RayXAppBuilder};
pub use crate::app::ready_ts::ReadyContext;
pub use crate::configuration::configuration_builder_ts::ConfigurationBuilder;
pub use crate::configuration::options_ts::{OptionsBuilder, OptionsToken};
pub use crate::configuration::options_ts::{push, record};
#[doc(hidden)]
pub use crate::di::contract_ts::__private;
pub use crate::di::contract_ts::{
    Contract, ContractError, ContractEvents, ContractRegistryExt, ContractState, Exception,
    contract_token,
};
pub use crate::di::provider_ts::{
    Resolved, ServiceClass, ServiceClassRegistryExt, ServiceKeyArg, ServiceProvider, ServiceToken,
};
pub use crate::di::startup_ts::{
    AsyncInitializable, JsAsyncInitializable, JsStartupInitializable, StartupInitializable,
    StartupInitializationContext, StartupInitializationState,
};
pub use crate::services::json;
pub use crate::services::listeners_ts::Listeners;

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

/// A component's previews: the states a design tool or a gallery shows it in, declared in a
/// `*.preview.tsx` file beside it as plain data typed with this alias (`export default { … }
/// satisfies Previews<typeof Card>`). Nothing runs to make it one: a tool reads the literal `id`,
/// `title`, `group`, state names and props without running the file, and imports it to show one.
pub const PREVIEWS: TsAlias = TsAlias {
    name: "Previews<C>",
    ts: "{ readonly id: string; readonly title?: string; readonly group?: string; \
         readonly component: C; readonly states: { readonly [name: string]: \
         PreviewState<C extends (props: infer P) => unknown ? P : never> } }",
    doc: "A component's previews, declared in its `*.preview.tsx` file: a stable `id` (unique in \
          the project, kept when the file moves), a `title` and `group` to list it under, the \
          `component`, and its named `states`.",
};

/// One named state of a component's previews ([`PREVIEWS`]): the props it is shown with.
pub const PREVIEW_STATE: TsAlias = TsAlias {
    name: "PreviewState<P>",
    ts: "{ readonly props?: Partial<P> }",
    doc: "A named state of a component's previews: the props it is shown with (those left out \
          keep the component's defaults).",
};

/// Adds RayX's TS classes to `registry` (an app's UI set): the builder, the provider and its
/// tokens (with the `ServiceKey<T>` alias), the previews' types (`Previews<C>`,
/// `PreviewState<P>`), the TS options' `OptionsToken` and
/// `OptionsBuilder`, the startup interfaces (`AsyncInitializable`,
/// `StartupInitializable`) and classes, the options classes and the services' classes.
pub fn register(registry: &mut Registry) {
    crate::devtools::register();
    registry
        .ts_alias(SERVICE_KEY)
        .ts_alias(CONTRACT_CLASS)
        .ts_alias(PREVIEWS)
        .ts_alias(PREVIEW_STATE)
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
        .class::<ConfigurationBuilder>();
}
