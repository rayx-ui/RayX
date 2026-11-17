//! The TS `RayXAppBuilder`: the app's composition root in TSX, a camelCase twin of the Rust
//! builder's methods, started with `await builder.start()`.

use std::time::Duration;

use nsh_core::prelude::*;
use rayx_app::{
    ConfigurationAppBuilderExt as _, RayXAppBuilder as AppBuilder, ServiceAppBuilderExt as _,
};
use rayx_runtime::services::application_configuration::ApplicationConfigurationAppBuilderExt as _;
use rayx_runtime::services::input_map::InputMapAppBuilderExt as _;
use rayx_runtime::services::selection::SelectionAppBuilderExt as _;

use crate::configuration::options_ts::record;
use crate::ts::{ApplicationConfigurationOptions, ConfigurationBuilder, InputMapOptions};
use std::collections::HashMap;

use nsh_core::JsonValue;
use rayx_app::di::RuntimeKey;

use crate::app::ready_ts as ready;
use crate::configuration::options_ts::{self as app_options, OptionsBuilder, OptionsToken};
use crate::di::container_ts::{self as container, Factory, TsService};
use crate::di::provider_ts::{ServiceKeyArg, ServiceProvider};
use crate::{AppError, AppSettingsAppBuilderExt as _};
use rayx_app::AppBuilder as _;
use rayx_app::di::{ServiceLifetime, StartupServiceType};

/// The app's composition root, for TSX: `new RayXAppBuilder()` (NSH's services first, as a Rust
/// root's start adds them), a chain of the Rust builder's methods in camelCase, then
/// `await builder.start()`:
///
/// ```ts
/// const services = await new RayXAppBuilder()
///   .addAppSettings("my-app")
///   .addApplicationConfiguration()
///   .start();
/// ```
///
/// Each method registers what the Rust method of its name does, and no other service (what it
/// needs it requires, and the start names what is missing); a service crate's set adds its own
/// methods (`addStorage` from `rayx_services_storage`). A method taking a callback calls it at once with an options object
/// whose methods are Rust's; what it sets applies after the options bound from configuration, and
/// a callback that throws makes the method throw. After `start()` the other methods throw.
///
/// # Rust
///
/// It wraps a `rayx_app::RayXAppBuilder`; a Rust bridge extends it with
/// [`RayXAppBuilder::extend`].
#[class]
pub struct RayXAppBuilder {
    /// The Rust builder until `start()` takes it.
    builder: Option<AppBuilder>,
    /// The TS options registered so far, by their tokens' keys.
    options: HashMap<RuntimeKey, app_options::SharedOptions>,
}

impl Default for RayXAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[class]
impl RayXAppBuilder {
    /// An empty builder for the app on this surface. Its start requires the platform,
    /// suspension, asset and font services (`addPlatform()`, `addSuspension()`, `addAssets()`,
    /// `addFonts()` from their crates' sets, or the app's own).
    pub fn new() -> Self {
        crate::devtools::register();
        Self {
            builder: Some(AppBuilder::new()),
            options: HashMap::new(),
        }
    }

    /// Adds `appsettings.json` from `package`'s `assets/` directory (the app's crate) as a
    /// configuration source: a missing file fails startup.
    pub fn add_app_settings(
        this: Instance<Self>,
        package: String,
    ) -> Result<Instance<Self>, AppError> {
        chain(this, |builder| builder.add_app_settings(&package))
    }

    /// Adds the JSON file at `uri` (`asset:/@my-app/appsettings.Development.json`) as a
    /// configuration source, adding nothing when it is missing.
    pub fn add_optional_app_settings(
        this: Instance<Self>,
        uri: String,
    ) -> Result<Instance<Self>, AppError> {
        chain(this, |builder| builder.add_optional_app_settings(uri))
    }

    /// Adds configuration sources: `configure` gets the configuration builder at once
    /// (`addJson`, `addInMemory`, `addValue`), and its sources come after those added before.
    pub fn configure_app_configuration(
        this: Instance<Self>,
        #[args(configuration)] configure: Callback<Instance<ConfigurationBuilder>>,
    ) -> Result<Instance<Self>, String> {
        let steps = record(
            ConfigurationBuilder::new(),
            Some(configure),
            ConfigurationBuilder::take,
        )?;
        chain_err(this, |builder| {
            builder.configure_app_configuration(|configuration| {
                for step in steps {
                    step.apply(configuration);
                }
            })
        })
    }

    /// Registers the app's input map (it keeps the user's bindings in the app's key-value store:
    /// `addStorage`); `configure` adds the app's bindings (`options.bindKey("Editor", "ctrl-s", "save")`) after
    /// configuration's options.
    pub fn add_input_map(
        this: Instance<Self>,
        #[args(options)] configure: Option<Callback<Instance<InputMapOptions>>>,
    ) -> Result<Instance<Self>, String> {
        let bindings = record(InputMapOptions::new(), configure, InputMapOptions::take)?;
        chain_err(this, |builder| {
            builder.add_input_map(move |options| {
                for binding in &bindings {
                    InputMapOptions::apply(binding, options);
                }
            })
        })
    }

    /// Registers the app's selection service.
    pub fn add_selection(this: Instance<Self>) -> Result<Instance<Self>, AppError> {
        chain(this, |builder| builder.add_selection())
    }

    /// Registers the app's configuration service (`RayX:Application`: its layout direction,
    /// motion and other settings) and the platform service it reads; `configure` sets its
    /// settings (`options.name("My App")`) after configuration's.
    pub fn add_application_configuration(
        this: Instance<Self>,
        #[args(options)] configure: Option<Callback<Instance<ApplicationConfigurationOptions>>>,
    ) -> Result<Instance<Self>, String> {
        let steps = record(
            ApplicationConfigurationOptions::new(),
            configure,
            ApplicationConfigurationOptions::take,
        )?;
        chain_err(this, |builder| {
            builder.add_application_configuration(move |settings| {
                for step in &steps {
                    step.apply(settings);
                }
            })
        })
    }

    /// The time, in milliseconds, a startup service that sets none of its own has to
    /// initialize before startup fails.
    pub fn default_initialization_timeout(
        this: Instance<Self>,
        ms: f64,
    ) -> Result<Instance<Self>, String> {
        let timeout = Duration::try_from_secs_f64(ms / 1000.0)
            .map_err(|_| format!("defaultInitializationTimeout takes milliseconds, not {ms}"))?;
        chain_err(this, |builder| {
            builder.default_initialization_timeout(timeout)
        })
    }

    /// Registers a singleton written in TS under `token` (a TS class, or a `ServiceToken` for an
    /// interface): `factory` makes it with the resolving `ServiceProvider` on its first resolve,
    /// and every scope of the root resolves that one object. It may resolve Rust services and
    /// other TS services.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_singleton(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(
            this,
            ServiceLifetime::Singleton,
            token,
            factory,
            &sys,
            false,
        )
    }

    /// [`addSingleton`](Self::add_singleton) for an implementation registered under an
    /// interface's token (`addSingletonAs(Clock, () => new SystemClock())`).
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_singleton_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(
            this,
            ServiceLifetime::Singleton,
            token,
            factory,
            &sys,
            false,
        )
    }

    /// Registers a scoped service written in TS under `token`: one object per scope (the root
    /// is a scope too), made by `factory` with that scope's `ServiceProvider` and released when
    /// the scope is disposed.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_scoped(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Scoped, token, factory, &sys, false)
    }

    /// [`addScoped`](Self::add_scoped) for an implementation registered under an interface's
    /// token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_scoped_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Scoped, token, factory, &sys, false)
    }

    /// Registers a transient service written in TS under `token`: `factory` makes a new object
    /// on every resolve.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_transient(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(
            this,
            ServiceLifetime::Transient,
            token,
            factory,
            &sys,
            false,
        )
    }

    /// [`addTransient`](Self::add_transient) for an implementation registered under an
    /// interface's token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_transient_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(
            this,
            ServiceLifetime::Transient,
            token,
            factory,
            &sys,
            false,
        )
    }

    /// Registers a startup service written in TS under `token`, a singleton: the one object `start()` initializes is the one every scope resolves. `factory` makes it
    /// in `start()`, which awaits its `initialize` (an `AsyncInitializable` or
    /// `StartupInitializable`) in dependency order with Rust's startup services, within its
    /// timeout; it is keyed by `token` unless its `startupServiceType()` names another.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_startup_singleton(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Singleton, token, factory, &sys, true)
    }

    /// [`addStartupSingleton`](Self::add_startup_singleton) for an implementation registered under an
    /// interface's token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_startup_singleton_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Singleton, token, factory, &sys, true)
    }

    /// Registers a startup service written in TS under `token`, a scoped service: `start()` initializes the root scope's object. `factory` makes it
    /// in `start()`, which awaits its `initialize` (an `AsyncInitializable` or
    /// `StartupInitializable`) in dependency order with Rust's startup services, within its
    /// timeout; it is keyed by `token` unless its `startupServiceType()` names another.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_startup_scoped(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Scoped, token, factory, &sys, true)
    }

    /// [`addStartupScoped`](Self::add_startup_scoped) for an implementation registered under an
    /// interface's token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_startup_scoped_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Scoped, token, factory, &sys, true)
    }

    /// Registers a startup service written in TS under `token`, a transient service: `start()` initializes an object of its own, and every resolve makes a new one. `factory` makes it
    /// in `start()`, which awaits its `initialize` (an `AsyncInitializable` or
    /// `StartupInitializable`) in dependency order with Rust's startup services, within its
    /// timeout; it is keyed by `token` unless its `startupServiceType()` names another.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_startup_transient(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Transient, token, factory, &sys, true)
    }

    /// [`addStartupTransient`](Self::add_startup_transient) for an implementation registered under an
    /// interface's token.
    #[ts("<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this")]
    #[ts("<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this")]
    pub fn add_startup_transient_as(
        this: Instance<Self>,
        token: ServiceKeyArg,
        #[args(services)] factory: Factory,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ts_service(this, ServiceLifetime::Transient, token, factory, &sys, true)
    }

    /// Registers options written in TS under `token` (an `OptionsToken`, JSON-shaped options
    /// with their defaults): `configure` gets an `OptionsBuilder` at once to bind a
    /// configuration section, configure, validate and validate them on start
    /// (`options.bind("Feed").validate(value => value.pageSize > 0, "pageSize must be
    /// positive")`). `services.getOptions(token)` resolves them.
    #[ts("<T>(token: OptionsToken<T>, configure?: (options: OptionsBuilder<T>) => void): this")]
    pub fn add_options(
        this: Instance<Self>,
        token: Instance<OptionsToken>,
        #[args(options)] configure: Option<Callback<Instance<OptionsBuilder>>>,
    ) -> Result<Instance<Self>, String> {
        let added = app_options::Added::recorded(configure)?;
        add_ts_options(this, &token, added)
    }

    /// Configures the TS options `token` names: `configure` gets the value and returns it
    /// configured, after the steps added before it.
    #[ts("<T>(token: OptionsToken<T>, configure: (value: T) => T): this")]
    pub fn configure_options(
        this: Instance<Self>,
        token: Instance<OptionsToken>,
        #[args(value)] configure: Callback<JsonValue, Option<JsonValue>>,
    ) -> Result<Instance<Self>, String> {
        add_ts_options(this, &token, app_options::Added::configure(configure))
    }

    /// Runs `hook` once every startup service initialized, before the app is ready: the ready
    /// hooks run one after another in the order added (Rust's, those of `apply`'d extensions
    /// included), and one that throws or rejects fails `start()` with its message.
    #[ts("(hook: (context: ReadyContext) => void | Promise<void>): this")]
    pub fn on_ready(
        this: Instance<Self>,
        #[args(context)] hook: ready::Hook,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ready_hook(this, None, hook, &sys)
    }

    /// [`onReady`](Self::on_ready), run only when `predicate` holds for the root
    /// `ServiceProvider` then.
    #[ts(
        "(predicate: (services: ServiceProvider) => boolean, hook: (context: ReadyContext) => void | Promise<void>): this"
    )]
    pub fn on_ready_when(
        this: Instance<Self>,
        #[args(services)] predicate: ready::Predicate,
        #[args(context)] hook: ready::Hook,
        sys: System,
    ) -> Result<Instance<Self>, String> {
        add_ready_hook(this, Some(predicate), hook, &sys)
    }

    /// Applies `extension` to the builder now, in chain order: an app's Rust extension exported
    /// to TS (`#[app_extension] fn add_weather(..)` is `addWeather`), or a TS function that
    /// chains the builder's methods. Throws what the extension throws.
    #[ts("(extension: (builder: RayXAppBuilder) => RayXAppBuilder): this")]
    pub fn apply(
        this: Instance<Self>,
        #[args(builder)] extension: Callback<Instance<RayXAppBuilder>, Instance<RayXAppBuilder>>,
    ) -> Result<Instance<Self>, String> {
        if this.is_disposed() || this.with(|builder| builder.builder.is_none()) {
            return Err(AppError::BuilderStarted.to_string());
        }
        extension.call(this.clone())?;
        Ok(this)
    }

    /// Starts the app on this surface: builds its services (loading their configuration),
    /// validates the options validated on start, initializes every startup service and runs
    /// the ready hooks, then resolves with the root `ServiceProvider`, and keeps the platform and
    /// suspension services current from NSH from then on. It rejects with the startup error's
    /// message. The builder is taken in the call itself, so its other methods throw from then
    /// on, in the same turn too. Once started (or while it starts), a second `start()` on the
    /// surface, of this builder or another, resolves with the same root.
    pub fn start(this: Instance<Self>, sys: System) -> Later<Instance<ServiceProvider>> {
        // In the JS call: nothing added after `start()` reaches the startup.
        let builder = if this.is_disposed() {
            None
        } else {
            this.with_mut(|this| this.builder.take())
        };
        Later::new(async move {
            let root = crate::app::surface::start_app_with(
                builder,
                crate::app::surface::StartLanguage::Ts,
            )
            .await?;
            crate::devtools::app_ready(Some(&sys));
            Ok::<_, AppError>(crate::di::provider_ts::root_of(&sys, root))
        })
    }
}

impl RayXAppBuilder {
    /// Applies `extend` to the Rust builder (what an app's Rust extension of the builder
    /// registers); fails once the builder started.
    pub fn extend(
        &mut self,
        extend: impl FnOnce(AppBuilder) -> AppBuilder,
    ) -> Result<(), AppError> {
        let builder = self.builder.take().ok_or(AppError::BuilderStarted)?;
        self.builder = Some(extend(builder));
        Ok(())
    }
}

impl RayXAppBuilder {
    /// `builder` with `extend` applied to its Rust builder, for chaining: what a service crate's
    /// TS builder method (a class extension of `RayXAppBuilder`) does with its Rust
    /// registration, as the builder's own methods do. Fails, for TS to throw, once the builder
    /// started.
    pub fn extend_with(
        builder: Instance<Self>,
        extend: impl FnOnce(AppBuilder) -> AppBuilder,
    ) -> Result<Instance<Self>, String> {
        chain_err(builder, extend)
    }

    /// `builder` with the Rust app extension `extension` applied: what the TS builder method
    /// `#[app_extension]` makes does.
    pub fn extended(
        builder: Instance<Self>,
        extension: fn(AppBuilder) -> AppBuilder,
    ) -> Result<Instance<Self>, String> {
        chain_err(builder, extension)
    }
}

/// `this` with `added` added to the TS options `token` names.
fn add_ts_options(
    this: Instance<RayXAppBuilder>,
    token: &Instance<OptionsToken>,
    added: app_options::Added,
) -> Result<Instance<RayXAppBuilder>, String> {
    let key = app_options::key_of(token);
    this.with_mut(|this| {
        let kept = this.options.get(&key).cloned();
        let builder = this
            .builder
            .as_mut()
            .ok_or_else(|| AppError::BuilderStarted.to_string())?;
        let shared = app_options::add(builder.services_mut(), kept, token, added)?;
        this.options.insert(key, shared);
        Ok::<_, String>(())
    })?;
    Ok(this)
}

/// `this` with the TS ready `hook` added.
fn add_ready_hook(
    this: Instance<RayXAppBuilder>,
    predicate: Option<ready::Predicate>,
    hook: ready::Hook,
    sys: &System,
) -> Result<Instance<RayXAppBuilder>, String> {
    let hook = ready::hook(predicate, hook, sys)?;
    chain_err(this, |builder| ready::add(builder, hook))
}

/// `this` with the TS service `factory` makes registered under `token` with `lifetime`.
fn add_ts_service(
    this: Instance<RayXAppBuilder>,
    lifetime: ServiceLifetime,
    token: ServiceKeyArg,
    factory: Factory,
    sys: &System,
    startup: bool,
) -> Result<Instance<RayXAppBuilder>, String> {
    if token.is_rayx_service() {
        return Err(format!(
            "{} is a RayX service's class: register a TS service under a TS class or a ServiceToken",
            token.name()
        ));
    }
    if this.is_disposed() || this.with(|builder| builder.builder.is_none()) {
        return Err(AppError::BuilderStarted.to_string());
    }
    let service = TsService::new(&token, factory, sys)?;
    let key = token.key(sys);
    let mut failed = None;
    chain_err(this, |mut builder| {
        let services = builder.services_mut();
        if let Err(error) = container::add(services, lifetime, service.clone()) {
            failed = Some(error);
            return builder;
        }
        let contract = token.contract();
        if let Some(contract) = contract {
            (contract.add)(services, lifetime, service.clone());
        }
        match (startup, contract) {
            (true, _) => crate::di::startup_ts::add(
                services,
                service,
                contract.map_or_else(
                    || StartupServiceType::runtime(&key),
                    |contract| (contract.startup)(),
                ),
                contract.map(|contract| contract.made),
            ),
            // Made by `start()`, so Rust resolves it synchronously from then on.
            (false, Some(contract)) if lifetime == ServiceLifetime::Singleton => {
                container::make_on_start(services, service, (contract.startup)(), contract.made);
            }
            (false, _) => {}
        }
        builder
    })
    .and_then(|this| failed.map_or(Ok(this), Err))
}

/// `this` with `extend` applied to its Rust builder, for chaining.
fn chain(
    this: Instance<RayXAppBuilder>,
    extend: impl FnOnce(AppBuilder) -> AppBuilder,
) -> Result<Instance<RayXAppBuilder>, AppError> {
    this.with_mut(|builder| builder.extend(extend))?;
    Ok(this)
}

/// [`chain`] for a member that fails with a message.
fn chain_err(
    this: Instance<RayXAppBuilder>,
    extend: impl FnOnce(AppBuilder) -> AppBuilder,
) -> Result<Instance<RayXAppBuilder>, String> {
    chain(this, extend).map_err(|error| error.to_string())
}
