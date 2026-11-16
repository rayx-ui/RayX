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
use std::sync::{Arc, LazyLock};

use dxn_reactive::{ActivatableViewModel, ActivationToken};
use nsh_core::element::{
    Arg, Class, JsHost, JsIn, JsOwned, JsValue, Local, PropError, Ret, TsTypes,
};
use nsh_core::prelude::*;
use rayx_core::di::{RuntimeKey, StartupServiceType};

use crate::AppError;

/// The app's services, for TSX: what `await builder.start()` resolves with (the surface's root),
/// and the scopes made from it.
///
/// `getRequiredService(I18nService)` resolves a RayX service by its class, `getService` gives
/// `null` when none is registered, `createScope()` makes a child scope (for a screen: make it
/// when the screen mounts, dispose it in its cleanup) and `dispose()` drops the scoped services
/// it resolved and deactivates the view models activated through it; using it afterwards
/// throws.
///
/// # Rust
///
/// Bridges take an `Instance<ServiceProvider>` and resolve in it:
/// `provider.with(|provider| provider.services().required_service::<MyService>())`, or
/// `provider.with_mut(|provider| provider.view_model::<dyn MyViewModel>())` for a view model
/// activated until the provider is disposed.
#[class]
pub struct ServiceProvider {
    provider: rayx_core::ServiceProvider,
    /// The view models activated through it, until it is disposed.
    activations: Vec<ActivationToken>,
    /// The class objects of what it resolved, by class and service: one per service.
    wrapped: HashMap<(TypeId, usize), Wrapped>,
}

/// A service class's instance and the service it wraps (held, so its address is not reused).
struct Wrapped {
    _service: rayx_core::di::ErasedService,
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
        token: Instance<super::OptionsToken>,
    ) -> Result<nsh_core::JsonValue, String> {
        if this.is_disposed() {
            return Err("ServiceProvider is disposed".into());
        }
        let services = this.with(|provider| provider.provider.clone());
        super::app_options::resolve(&services, &token)
    }

    /// A child scope: it shares this provider's singletons and has scoped services of its own.
    /// Dispose it when what it serves (a screen) goes.
    pub fn create_scope(&self) -> Instance<ServiceProvider> {
        Instance::new(Self::of(self.provider.create_scope()))
    }
}

impl ServiceProvider {
    /// A TS provider over `provider`.
    pub(crate) fn of(provider: rayx_core::ServiceProvider) -> Self {
        Self {
            provider,
            activations: Vec::new(),
            wrapped: HashMap::new(),
        }
    }

    /// The Rust provider, for Rust bridges that take an `Instance<ServiceProvider>`.
    pub fn services(&self) -> &rayx_core::ServiceProvider {
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
        .and_then(|class| RESOLVERS.get(&class))
    {
        return (resolve.resolve)(this, required);
    }
    let key = sys.locals.get::<ServiceTokens>().key(&token.handle);
    let services = this.with(|provider| provider.provider.clone());
    match key {
        Some(key) => match services.required_thread_bound_service_by_key(&key) {
            Ok(service) => super::container::object(service, &key).map(Resolved::object),
            Err(_) if !required && !services.contains_service_by_key(&key) => Ok(Resolved::null()),
            Err(error) => Err(error.to_string()),
        },
        None if required => Err(format!("no service is registered for {}", token.name)),
        None => Ok(Resolved::null()),
    }
}

// --- RayX's service classes --------------------------------------------------------------

/// A RayX service's TS class: its object wraps the Rust service.
pub(crate) trait ServiceClass: Class {
    /// The Rust service.
    type Service: ?Sized + Send + Sync + 'static;
    /// The class's value for `service`.
    fn wrap(service: Arc<Self::Service>) -> Self;
}

/// A RayX service class as a token: how it resolves its Rust service in a provider (and gives
/// its object), and the Rust service's startup key.
struct ClassToken {
    resolve: fn(&Instance<ServiceProvider>, bool) -> Result<Resolved, String>,
    startup: fn() -> StartupServiceType,
}

/// RayX's service classes by their constructors (what a token that is one names).
static RESOLVERS: LazyLock<HashMap<Constructor, ClassToken>> = LazyLock::new(|| {
    use super::services::{
        ApplicationConfigurationService, AssetService, FontService, KeyValueStore,
        NavigationService, PlatformInfoService, SuspensionService,
    };
    fn entry<C: ServiceClass>() -> (Constructor, ClassToken) {
        (
            Constructor::of::<C>(),
            ClassToken {
                resolve: resolve_class::<C>,
                startup: StartupServiceType::of::<C::Service>,
            },
        )
    }
    HashMap::from([
        entry::<super::I18nService>(),
        entry::<NavigationService>(),
        entry::<KeyValueStore>(),
        entry::<ApplicationConfigurationService>(),
        entry::<AssetService>(),
        entry::<FontService>(),
        entry::<PlatformInfoService>(),
        entry::<SuspensionService>(),
    ])
});

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
    let key = (
        TypeId::of::<C>(),
        Arc::as_ptr(&service).cast::<()>() as usize,
    );
    let instance = this.with_mut(|provider| {
        let kept = provider
            .wrapped
            .get(&key)
            .and_then(|wrapped| wrapped.instance.downcast_ref::<Instance<C>>())
            .filter(|instance| !instance.is_disposed())
            .cloned();
        kept.unwrap_or_else(|| {
            let instance = Instance::new(C::wrap(Arc::clone(&service)));
            provider.wrapped.insert(
                key,
                Wrapped {
                    _service: Arc::new(service),
                    instance: Box::new(instance.clone()),
                },
            );
            instance
        })
    });
    Ok(Resolved::instance(instance))
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
    provider: &rayx_core::ServiceProvider,
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
struct SurfaceRoot(RefCell<Option<(Arc<rayx_core::Configuration>, Instance<ServiceProvider>)>>);

impl Local for SurfaceRoot {
    fn close(&self) {
        self.0.borrow_mut().take();
    }
}

/// `root`'s TS object in the VM `sys` is of: the same one each time (a root is told apart by
/// its configuration, a singleton of its own), unless JS disposed it.
pub(crate) fn root_of(sys: &System, root: rayx_core::ServiceProvider) -> Instance<ServiceProvider> {
    let slot = sys.locals.get::<SurfaceRoot>();
    let configuration = root.service::<rayx_core::Configuration>();
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
    contract: Option<super::contract::ContractDef>,
}

impl ServiceKeyArg {
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
    pub(crate) fn contract(&self) -> Option<super::contract::ContractDef> {
        self.contract
    }

    /// Whether the token is a RayX service's class (`I18nService`), which names that Rust
    /// service.
    pub(crate) fn is_rayx_service(&self) -> bool {
        self.handle
            .constructor()
            .is_some_and(|class| RESOLVERS.contains_key(&class))
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
        match self
            .handle
            .constructor()
            .and_then(|class| RESOLVERS.get(&class))
        {
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
            .and_then(|token| token.with(|token| token.contract));
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
    contract: Option<super::contract::ContractDef>,
}

impl ServiceToken {
    /// A contract's token.
    pub(crate) fn contract(name: String, contract: super::contract::ContractDef) -> Self {
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
