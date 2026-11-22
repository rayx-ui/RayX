//! The TS `ServiceProvider`: what `await builder.start()` resolves with, and its scopes.
//!
//! A token is a JS object ([`JsHandle`]). A RayX service class (`I18nService`) is its own token:
//! its constructor keys [`RESOLVERS`], which resolves the Rust service and wraps it in its class,
//! one object per resolved service per provider (so `===` holds for a singleton). Any other
//! object (a TS class, a `ServiceToken`) is looked up by identity in the surface's
//! [`ServiceTokens`], which gives the runtime key it is registered under.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use dxn_reactive::{ActivatableViewModel, ActivationToken};
use nsh_core::element::{
    Arg, Class, JsHost, JsIn, JsOwned, JsValue, Local, PropError, Ret, TsTypes,
};
use nsh_core::prelude::*;
use rayx_app::di::{RuntimeKey, StartupServiceType};

use crate::AppError;

/// The app's services, for TSX: what `await builder.start()` resolves with (the surface's root),
/// and the scopes made from it.
///
/// `getRequiredService(I18nService)` resolves a RayX service by its class, `getService` gives
/// `null` when none is registered, `createScope()` makes a child scope (for a screen: make it
/// when the screen mounts, dispose it in its cleanup), `createScope(configure)` one with
/// registrations of its own, and `dispose()` drops the scoped services
/// it resolved and deactivates the view models activated through it; using it afterwards
/// throws. The singletons a scope of its own registered live on while one of its descendants
/// does, made over its registrations ([`ScopeRegistrations`](crate::ts::ScopeRegistrations)).
///
/// # Rust
///
/// Bridges take an `Instance<ServiceProvider>` and resolve in it:
/// `provider.with(|provider| provider.services().required_service::<MyService>())`, or
/// `provider.with_mut(|provider| provider.view_model::<dyn MyViewModel>())` for a view model
/// activated until the provider is disposed.
#[class]
pub struct ServiceProvider {
    provider: rayx_app::ServiceProvider,
    /// The view models activated through it, until it is disposed.
    activations: Vec<ActivationToken>,
    /// The class objects of what it resolved, by class and service: one per service.
    wrapped: HashMap<(TypeId, usize), Wrapped>,
}

/// A service class's instance and the service it wraps (held, so its address is not reused).
struct Wrapped {
    _service: rayx_app::di::ErasedService,
    instance: Box<dyn Any>,
}

#[class]
impl ServiceProvider {
    /// The service `token` names (a RayX service's class, such as `I18nService`, or a
    /// `ServiceToken`), or `null` when none is registered under it.
    #[ts("<T>(token: ServiceKey<T>): T | null")]
    pub fn get_service(
        this: Instance<Self>,
        token: ServiceKeyArg,
        sys: System,
    ) -> Result<Resolved, String> {
        resolve(&this, &token, &sys, false)
    }

    /// The service `token` names (a RayX service's class, such as `I18nService`, or a
    /// `ServiceToken`); throws an `Error` naming the token when none is registered under it.
    #[ts("<T>(token: ServiceKey<T>): T")]
    pub fn get_required_service(
        this: Instance<Self>,
        token: ServiceKeyArg,
        sys: System,
    ) -> Result<Resolved, String> {
        resolve(&this, &token, &sys, true)
    }

    /// The TS options `token` names, made now: the token's defaults, the bound configuration
    /// section, then the configure callbacks in order; throws when a validation fails (with
    /// every failing validation's message).
    #[ts("<T>(token: OptionsToken<T>): { value: T }")]
    pub fn get_options(
        this: Instance<Self>,
        token: Instance<crate::ts::OptionsToken>,
    ) -> Result<nsh_core::JsonValue, String> {
        if this.is_disposed() {
            return Err("ServiceProvider is disposed".into());
        }
        let services = this.with(|provider| provider.provider.clone());
        crate::configuration::options_ts::resolve(&services, &token)
    }

    /// A child scope: it shares this provider's singletons and has scoped services of its own.
    /// Dispose it when what it serves (a screen) goes.
    ///
    /// With `configure`, called at once with the scope's `ScopeRegistrations`, the scope has
    /// registrations of its own: for each token registered there, they win in the scope and
    /// its descendants over this provider's (also once it made that token), its singletons
    /// are one per such scope and always made with it (also once it is disposed, for a
    /// descendant that resolves one first), and this provider and its other scopes resolve as
    /// before. The
    /// root's scoped and transient services resolved in it see its replacements; the root's
    /// singletons are made with the root. Throws what `configure` throws.
    #[ts("(configure?: (scope: ScopeRegistrations) => void): ServiceProvider")]
    pub fn create_scope(
        this: Instance<Self>,
        #[args(scope)] configure: Option<
            Callback<Instance<crate::di::scope_ts::ScopeRegistrations>>,
        >,
    ) -> Result<Instance<ServiceProvider>, String> {
        if this.is_disposed() {
            return Err("ServiceProvider is disposed".into());
        }
        let services = this.with(|provider| provider.provider.clone());
        let Some(configure) = configure else {
            return Ok(Instance::new(Self::of(services.create_scope())));
        };
        let recorded = crate::configuration::options_ts::record(
            crate::di::scope_ts::ScopeRegistrations::new(),
            Some(configure),
            crate::di::scope_ts::ScopeRegistrations::take,
        )?;
        let mut failed = None;
        let scope = services.create_scope_with(|scope| {
            failed = crate::di::scope_ts::apply(scope, recorded).err();
        });
        match failed {
            Some(error) => Err(error),
            None => Ok(Instance::new(Self::of(scope))),
        }
    }
}

impl ServiceProvider {
    /// A TS provider over `provider`.
    pub(crate) fn of(provider: rayx_app::ServiceProvider) -> Self {
        Self {
            provider,
            activations: Vec::new(),
            wrapped: HashMap::new(),
        }
    }

    /// The Rust provider, for Rust bridges that take an `Instance<ServiceProvider>`.
    pub fn services(&self) -> &rayx_app::ServiceProvider {
        &self.provider
    }

    /// `T` resolved in this provider and activated until it is disposed.
    pub fn view_model<T>(&mut self) -> Result<Arc<T>, AppError>
    where
        T: ?Sized + ActivatableViewModel + Send + Sync + 'static,
    {
        let view_model = self.provider.required_service::<T>()?;
        self.activations.push(view_model.activator().activate()?);
        Ok(view_model)
    }
}

/// `token` resolved in `this`: `None` (`null`) when nothing is registered under it, unless it is
/// `required`.
fn resolve(
    this: &Instance<ServiceProvider>,
    token: &ServiceKeyArg,
    sys: &System,
    required: bool,
) -> Result<Resolved, String> {
    if this.is_disposed() {
        return Err("ServiceProvider is disposed".into());
    }
    // A TS factory this resolve runs gets `this`.
    let _resolving = Resolving::push(this);
    if let Some(resolve) = token
        .handle
        .constructor()
        .and_then(|class| class_of(&class))
    {
        return (resolve.resolve)(this, required);
    }
    let key = sys.locals.get::<ServiceTokens>().key(&token.handle);
    let services = this.with(|provider| provider.provider.clone());
    match key {
        Some(key) => match services.required_thread_bound_service_by_key(&key) {
            Ok(service) => crate::di::container_ts::object(service, &key).map(Resolved::object),
            Err(_) if !required && !services.contains_service_by_key(&key) => Ok(Resolved::null()),
            Err(error) => Err(error.to_string()),
        },
        None if required => Err(format!("no service is registered for {}", token.name)),
        None => Ok(Resolved::null()),
    }
}

// --- RayX's service classes --------------------------------------------------------------

/// A RayX service's TS class: its object wraps the Rust service, and the class is the
/// service's token in TS (`services.getRequiredService(I18nService)`).
///
/// A set registers its classes (a contract's class through
/// [`ContractRegistryExt::contract_class`](crate::ts::ContractRegistryExt::contract_class)); the
/// provider then resolves the class to the one object it gives for the Rust service registered
/// as [`Service`](Self::Service).
pub trait ServiceClass: Class {
    /// The Rust service.
    type Service: ?Sized + Send + Sync + 'static;
    /// The class's value for `service`.
    fn wrap(service: Arc<Self::Service>) -> Self;

    /// The class's value for `service`, resolved in `services`: what the provider makes it with.
    /// A class over more than its service (the i18n class's formatter) resolves the rest here;
    /// by default it is [`wrap`](Self::wrap).
    fn wrap_in(service: Arc<Self::Service>, services: &rayx_app::ServiceProvider) -> Self {
        let _ = services;
        Self::wrap(service)
    }
}

/// A RayX service class as a token: how it resolves its Rust service in a provider (and gives
/// its object), the Rust service's startup key, and the contract TS implements under it, when
/// it is a contract's class.
#[derive(Clone, Copy)]
struct ClassToken {
    resolve: fn(&Instance<ServiceProvider>, bool) -> Result<Resolved, String>,
    startup: fn() -> StartupServiceType,
    contract: Option<crate::di::contract_ts::ContractDef>,
    /// Activates the class's Rust view model, when it is a view model's class
    /// ([`ServiceClassRegistryExt::view_model_class`]).
    activate: Option<ActivateClass>,
}

/// How a view model's class activates its Rust view model in a provider.
type ActivateClass = fn(&Instance<ServiceProvider>) -> Result<(Resolved, ActivationToken), String>;

fn class_token<C: ServiceClass>(
    contract: Option<crate::di::contract_ts::ContractDef>,
) -> ClassToken {
    ClassToken {
        resolve: resolve_class::<C>,
        startup: StartupServiceType::of::<C::Service>,
        contract,
        activate: None,
    }
}

/// `C`'s Rust view model resolved in `this` once, wrapped in its class's object, and that very
/// instance activated, as [`ServiceProvider::view_model`] activates it (the token is the caller's
/// to keep): a transient view model is activated as the object JS gets.
fn activate_class<C>(
    this: &Instance<ServiceProvider>,
) -> Result<(Resolved, ActivationToken), String>
where
    C: ServiceClass,
    C::Service: ActivatableViewModel,
{
    let services = this.with(|provider| provider.provider.clone());
    let view_model = services
        .required_service::<C::Service>()
        .map_err(|error| error.to_string())?;
    let token = view_model
        .activator()
        .activate()
        .map_err(|error| error.to_string())?;
    Ok((wrap_class::<C>(this, &services, view_model), token))
}

/// The service classes sets registered ([`ServiceClassRegistryExt::service_class`],
/// [`ContractRegistryExt::contract_class`](crate::ts::ContractRegistryExt::contract_class)).
static REGISTERED: LazyLock<RwLock<HashMap<Constructor, ClassToken>>> =
    LazyLock::new(RwLock::default);

/// Registers `C` as a service class: the provider resolves it to its Rust service, and, with
/// `contract`, TS registrations under it implement that contract.
pub(crate) fn register_class<C: ServiceClass>(
    contract: Option<crate::di::contract_ts::ContractDef>,
) {
    REGISTERED
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(Constructor::of::<C>(), class_token::<C>(contract));
}

/// The class token of the class `class`.
fn class_of(class: &Constructor) -> Option<ClassToken> {
    REGISTERED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(class)
        .copied()
}

/// Service classes in an app's set: `registry.service_class::<WeatherService>()` adds the
/// class and makes it its Rust service's token, which `getService` and `getRequiredService`
/// resolve to the one object the provider gives for the service. What a service crate's set
/// registers for each of its services' classes.
pub trait ServiceClassRegistryExt {
    /// Adds the service class `C` and its resolver.
    fn service_class<C: ServiceClass>(&mut self) -> &mut Self;

    /// Adds the class `C` of a Rust view model: a service class whose service TS's
    /// `useViewModel(C)` activates for as long as the calling component lives.
    fn view_model_class<C>(&mut self) -> &mut Self
    where
        C: ServiceClass,
        C::Service: ActivatableViewModel;
}

impl ServiceClassRegistryExt for nsh_core::element::Registry {
    fn service_class<C: ServiceClass>(&mut self) -> &mut Self {
        register_class::<C>(None);
        self.class::<C>()
    }

    fn view_model_class<C>(&mut self) -> &mut Self
    where
        C: ServiceClass,
        C::Service: ActivatableViewModel,
    {
        let mut token = class_token::<C>(None);
        token.activate = Some(activate_class::<C>);
        REGISTERED
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(Constructor::of::<C>(), token);
        self.class::<C>()
    }
}

/// The view model `token` names, resolved in `this` as `getRequiredService` resolves it, and
/// activated: a Rust view model's class ([`ServiceClassRegistryExt::view_model_class`]) as
/// [`ServiceProvider::view_model`] activates it, a TS object through its `activator`. Errors
/// name the token; nothing is activated then.
pub(crate) fn resolve_view_model(
    this: &Instance<ServiceProvider>,
    token: &ServiceKeyArg,
    sys: &System,
) -> Result<(Resolved, crate::di::view_model_ts::Activation), String> {
    use crate::di::view_model_ts::{Activation, activate_object, not_a_view_model};
    if let Some(class) = token
        .handle
        .constructor()
        .and_then(|class| class_of(&class))
    {
        let activate = class
            .activate
            .ok_or_else(|| not_a_view_model(&token.name))?;
        if this.is_disposed() {
            return Err("ServiceProvider is disposed".into());
        }
        // A TS factory this resolve runs gets `this`.
        let _resolving = Resolving::push(this);
        let (resolved, activation) = activate(this)?;
        return Ok((resolved, Activation::Rust(activation)));
    }
    let resolved = resolve(this, token, sys, true)?;
    match &resolved.0 {
        Inner::Object(object) => {
            let activation = activate_object(object, &token.name)?;
            Ok((resolved, activation))
        }
        _ => Err(not_a_view_model(&token.name)),
    }
}

/// `C`'s service resolved in `this`, as the one object of `C` this provider gives for it.
fn resolve_class<C: ServiceClass>(
    this: &Instance<ServiceProvider>,
    required: bool,
) -> Result<Resolved, String> {
    let services = this.with(|provider| provider.provider.clone());
    if !required && !services.contains_service::<C::Service>() {
        return Ok(Resolved::null());
    }
    let service = services
        .required_service::<C::Service>()
        .map_err(|error| error.to_string())?;
    Ok(wrap_class::<C>(this, &services, service))
}

/// `service`, resolved in `this`'s `services`, as the one object of `C` this provider gives for
/// it.
fn wrap_class<C: ServiceClass>(
    this: &Instance<ServiceProvider>,
    services: &rayx_app::ServiceProvider,
    service: Arc<C::Service>,
) -> Resolved {
    let key = (
        TypeId::of::<C>(),
        Arc::as_ptr(&service).cast::<()>() as usize,
    );
    let kept = this.with(|provider| {
        provider
            .wrapped
            .get(&key)
            .and_then(|wrapped| wrapped.instance.downcast_ref::<Instance<C>>())
            .filter(|instance| !instance.is_disposed())
            .cloned()
    });
    if let Some(instance) = kept {
        return Resolved::instance(instance);
    }
    // Made outside the provider's borrow: the class may resolve more services.
    let instance = Instance::new(C::wrap_in(Arc::clone(&service), services));
    this.with_mut(|provider| {
        provider.wrapped.insert(
            key,
            Wrapped {
                _service: Arc::new(service),
                instance: Box::new(instance.clone()),
            },
        );
    });
    Resolved::instance(instance)
}

thread_local! {
    /// The TS providers resolving now, innermost last: a TS factory a resolve runs gets the one
    /// of its scope.
    static RESOLVING: RefCell<Vec<Instance<ServiceProvider>>> = const { RefCell::new(Vec::new()) };
}

/// A TS provider on [`RESOLVING`] for as long as it lives.
struct Resolving;

impl Resolving {
    fn push(provider: &Instance<ServiceProvider>) -> Self {
        RESOLVING.with_borrow_mut(|resolving| resolving.push(provider.clone()));
        Self
    }
}

impl Drop for Resolving {
    fn drop(&mut self) {
        let popped = RESOLVING.with_borrow_mut(Vec::pop);
        // Dropped outside the stack's borrow.
        drop(popped);
    }
}

/// The TS provider a TS factory resolving in `provider` gets: the one resolving now in its
/// scope, else the surface's root when `provider` is the root's scope, else a new one over it.
pub(crate) fn provider_for(
    sys: &System,
    provider: &rayx_app::ServiceProvider,
) -> Instance<ServiceProvider> {
    let resolving = RESOLVING.with_borrow(|resolving| {
        resolving
            .iter()
            .rev()
            .find(|candidate| {
                !candidate.is_disposed()
                    && candidate.with(|candidate| candidate.provider.same_scope(provider))
            })
            .cloned()
    });
    if let Some(resolving) = resolving {
        return resolving;
    }
    let kept = sys.locals.get::<SurfaceRoot>();
    let root = kept
        .0
        .borrow()
        .as_ref()
        .map(|(_, root)| root.clone())
        .filter(|root| !root.is_disposed() && root.with(|root| root.provider.same_scope(provider)));
    root.unwrap_or_else(|| Instance::new(ServiceProvider::of(provider.clone())))
}

/// The surface's root as its one TS object, for every `start()` that resolves with it.
#[derive(Default)]
struct SurfaceRoot(RefCell<Option<(Arc<rayx_app::Configuration>, Instance<ServiceProvider>)>>);

impl Local for SurfaceRoot {
    fn close(&self) {
        self.0.borrow_mut().take();
    }
}

/// `root`'s TS object in the VM `sys` is of: the same one each time (a root is told apart by
/// its configuration, a singleton of its own), unless JS disposed it.
pub(crate) fn root_of(sys: &System, root: rayx_app::ServiceProvider) -> Instance<ServiceProvider> {
    let slot = sys.locals.get::<SurfaceRoot>();
    let configuration = root.service::<rayx_app::Configuration>();
    let mut kept = slot.0.borrow_mut();
    if let (Some((kept_configuration, instance)), Some(configuration)) =
        (kept.as_ref(), configuration.as_ref())
        && Arc::ptr_eq(kept_configuration, configuration)
        && !instance.is_disposed()
    {
        return instance.clone();
    }
    let instance = Instance::new(ServiceProvider::of(root));
    if let Some(configuration) = configuration {
        *kept = Some((configuration, instance.clone()));
    }
    instance
}

// --- Tokens -------------------------------------------------------------------------------

/// The surface's TS tokens (TS classes, `ServiceToken`s) by identity, with the runtime keys
/// their services are registered under: one table per VM (each engine surface has its own).
#[derive(Default)]
pub(crate) struct ServiceTokens {
    keys: RefCell<HashMap<JsHandle, RuntimeKey>>,
}

impl ServiceTokens {
    /// The runtime key `token` names, if a service was registered under it.
    pub(crate) fn key(&self, token: &JsHandle) -> Option<RuntimeKey> {
        self.keys.borrow().get(token).cloned()
    }

    /// The runtime key `token` names, made (with `name`) the first time.
    pub(crate) fn key_or_insert(&self, token: &JsHandle, name: &str) -> RuntimeKey {
        self.keys
            .borrow_mut()
            .entry(token.clone())
            .or_insert_with(|| RuntimeKey::new(name.to_owned()))
            .clone()
    }
}

impl Local for ServiceTokens {
    fn close(&self) {
        // The handles go before the VM's values do.
        self.keys.borrow_mut().clear();
    }
}

/// A token TS names a service by: a class (RayX's, or one TS declares), a `ServiceToken` or any
/// other object, with a name for messages.
#[derive(Clone)]
pub struct ServiceKeyArg {
    handle: JsHandle,
    name: String,
    /// The contract the token is of (a contract's `ServiceToken`).
    contract: Option<crate::di::contract_ts::ContractDef>,
}

impl ServiceKeyArg {
    /// `handle` (a factory's function) as the token of a TS implementation of contract `C`
    /// registered by a builder method ([`crate::ts::ContractFactory`]).
    pub(crate) fn of_contract<C: ?Sized + crate::di::contract_ts::Contract>(
        handle: JsHandle,
    ) -> Self {
        Self {
            handle,
            name: C::NAME.to_owned(),
            contract: Some(crate::di::contract_ts::ContractDef::of::<C>()),
        }
    }

    /// The token's JS object.
    pub fn handle(&self) -> &JsHandle {
        &self.handle
    }

    /// What names the token in messages: a class's name, a `ServiceToken`'s
    /// (`ServiceToken(clock)`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The contract the token is of.
    pub(crate) fn contract(&self) -> Option<crate::di::contract_ts::ContractDef> {
        self.contract
    }

    /// Whether the token is a RayX service's class (`I18nService`), which names that Rust
    /// service.
    pub(crate) fn is_rayx_service(&self) -> bool {
        self.contract.is_none()
            && self
                .handle
                .constructor()
                .is_some_and(|class| class_of(&class).is_some())
    }

    /// The runtime key the token names in the VM `sys` is of, made the first time: what a TS
    /// service is registered under, and what Rust code keys a service TS names by this token
    /// (a Rust startup service's `startup_service_type`).
    pub fn key(&self, sys: &System) -> RuntimeKey {
        self.key_or_insert(sys)
    }

    pub(crate) fn key_or_insert(&self, sys: &System) -> RuntimeKey {
        sys.locals
            .get::<ServiceTokens>()
            .key_or_insert(&self.handle, &self.name)
    }

    /// What the token names in the startup run: a RayX service's class its Rust service's type
    /// (`I18nService`: the i18n service), any other token its runtime key.
    pub(crate) fn startup_type(&self, sys: &System) -> StartupServiceType {
        if let Some(contract) = self.contract {
            return (contract.startup)();
        }
        match self.handle.constructor().and_then(|class| class_of(&class)) {
            Some(class) => (class.startup)(),
            None => StartupServiceType::runtime(&self.key_or_insert(sys)),
        }
    }
}

impl ServiceProvider {
    /// The runtime key the TS token `token` (a TS class, a `ServiceToken`) names in the VM `sys`
    /// is of, once a TS service was registered under it: what Rust code resolves that service by
    /// (`thread_bound_service_by_key`, on the engine thread).
    pub fn key_of(sys: &System, token: &JsHandle) -> Option<RuntimeKey> {
        sys.locals.get::<ServiceTokens>().key(token)
    }
}

/// A token is a JS object (what TS passes, and what an interface's member returns).
impl JsValue for ServiceKeyArg {
    fn ts_type(_types: &mut TsTypes) -> String {
        "object".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let handle = <JsHandle as JsValue>::from_js(value)?;
        let token = <Instance<ServiceToken> as Arg>::from_js(value).ok();
        let contract = token
            .as_ref()
            .and_then(|token| token.with(|token| token.contract))
            .or_else(|| {
                handle
                    .constructor()
                    .and_then(|class| class_of(&class))
                    .and_then(|class| class.contract)
            });
        let name = match (handle.constructor(), token) {
            (Some(class), _) => class.def().name.to_owned(),
            (None, Some(token)) => token.with(|token| match token.contract {
                Some(_) => token.name.clone(),
                None => format!("ServiceToken({})", token.name),
            }),
            (None, None) => token_name(value),
        };
        Ok(Self {
            handle,
            name,
            contract,
        })
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        self.handle.to_js(host)
    }
}

/// What names `value` (no `ServiceToken`) in a message: a function's (a TS class's) name, else
/// what it is.
fn token_name(value: JsIn<'_>) -> String {
    value
        .field("name")
        .ok()
        .and_then(|name| name.get().as_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| value.describe())
}

/// A key for services that have no Rust type of their own (a service written in TS): a name,
/// and the type `T` of what it resolves to, for TypeScript.
#[class(generic = "T")]
pub struct ServiceToken {
    name: String,
    /// The contract it is the token of (`#[contract]`'s token function makes it).
    contract: Option<crate::di::contract_ts::ContractDef>,
}

impl ServiceToken {
    /// A contract's token.
    pub(crate) fn contract(name: String, contract: crate::di::contract_ts::ContractDef) -> Self {
        Self {
            name,
            contract: Some(contract),
        }
    }
}

#[class]
impl ServiceToken {
    /// A new token named `name` (the name is for messages: two tokens are two keys).
    pub fn new(name: String) -> Self {
        Self {
            name,
            contract: None,
        }
    }

    /// The name it was made with.
    #[getter]
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

// --- What a lookup gives ------------------------------------------------------------------

/// What a lookup gives JS: a service class's object, a JS object a TS service is, or `null`.
pub struct Resolved(Inner);

/// Makes a class's object for the host.
type ToObject = Box<dyn for<'a> FnOnce(&'a dyn JsHost) -> Result<JsOwned<'a>, PropError>>;

enum Inner {
    Null,
    Object(JsHandle),
    Instance(ToObject),
}

impl Resolved {
    fn null() -> Self {
        Self(Inner::Null)
    }

    fn object(handle: JsHandle) -> Self {
        Self(Inner::Object(handle))
    }

    fn instance<C: Class>(instance: Instance<C>) -> Self {
        Self(Inner::Instance(Box::new(move |host| {
            instance.into_js(host)
        })))
    }
}

impl Ret for Resolved {
    fn ts_type(_types: &mut TsTypes) -> String {
        "object | null".into()
    }

    fn into_js<'a>(self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self.0 {
            Inner::Null => Ok(JsOwned::new(host, host.null())),
            Inner::Object(handle) => handle.to_js(host),
            Inner::Instance(make) => make(host),
        }
    }
}
