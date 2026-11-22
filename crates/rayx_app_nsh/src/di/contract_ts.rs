//! Contracts: Rust traits TS implements (`#[contract]`), which Rust resolves as `Arc<dyn Trait>`.
//!
//! `#[contract]` keeps the trait `dyn`-compatible for Rust and makes, beside it, the NSH
//! interface of the trait's name (its stand-in `Js<Trait>` calls the JS object), the token TS
//! registers an implementation under (a flat function `<trait>Token()` giving the contract's
//! `ServiceToken`) and an adapter: `Send + Sync`, it implements the trait over the stand-in kept
//! on the engine thread ([`ContractSlot`]). Registering a TS implementation under the token
//! (`addSingletonAs(weatherSourceToken(), () => new OpenMeteo())`, and the other forms) also
//! registers the adapter as `dyn Trait`, with the same lifetime; a singleton (and every startup
//! form) is made during `start()`, before any Rust startup service's factory runs, so Rust
//! resolves it synchronously from then on (such a factory may read it), and Rust startup
//! services depending on `dyn Trait` initialize after it.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use nsh_core::JsonValue;
use nsh_core::element::{Local, Registry};
use nsh_core::prelude::*;
use rayx_app::di::{
    RuntimeKey, ServiceLifetime, ServiceResolutionError, StartupServiceType, ThreadBound,
    WeakServiceProvider,
};
use rayx_app::{EventStream, StateCell};

use crate::binding::EngineRunner;
use crate::di::container_ts::SharedTsService;
use crate::di::provider_ts::{ServiceClass, ServiceToken};

pub use nsh_core::element::Exception;

/// Why a contract's call did not reach its TS implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractError {
    /// The surface's engine stopped (or never ran a dispatcher) before the call reached its TS
    /// implementation.
    EngineGone { contract: &'static str },
    /// The TS implementation could not be resolved when the adapter was made (`message` says
    /// why).
    Unavailable {
        contract: &'static str,
        message: String,
    },
    /// A member the adapter reads once, when it is made (published state or events, a
    /// constant), is missing or does not convert (`message` says why).
    Member {
        contract: &'static str,
        member: &'static str,
        message: String,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineGone { contract } => write!(
                f,
                "`{contract}` is implemented in TS, and the surface's engine is gone"
            ),
            Self::Unavailable { contract, message } => {
                write!(
                    f,
                    "`{contract}` has no TS implementation to call: {message}"
                )
            }
            Self::Member {
                contract,
                member,
                message,
            } => write!(
                f,
                "`{contract}.{member}` of its TS implementation: {message}"
            ),
        }
    }
}

impl std::error::Error for ContractError {}

/// Its message: a contract whose methods fail with `String` takes it.
impl From<ContractError> for String {
    fn from(error: ContractError) -> Self {
        error.to_string()
    }
}

/// A Rust trait TS implements: what `#[contract]` implements for `dyn Trait`.
pub trait Contract: 'static {
    /// The trait object Rust resolves (`dyn WeatherSource`): `Self` for a contract of the app's
    /// crate, the core's trait for a marker `contract_nsh!` makes.
    type Dyn: ?Sized + Send + Sync + 'static;

    /// The trait's name.
    const NAME: &'static str;

    /// Its TS interface's name (`#[contract(ts = "…")]`, else the trait's).
    const INTERFACE: &'static str;

    /// Adds the contract's interface and token to an app's set.
    #[doc(hidden)]
    fn register(registry: &mut Registry);

    /// Adds the contract's interface alone to an app's set (its token is a class).
    #[doc(hidden)]
    fn register_interface(registry: &mut Registry);

    /// The adapter over the TS implementation `source` gives.
    #[doc(hidden)]
    fn adapter(source: __private::ContractSource) -> Arc<Self::Dyn>;

    /// Links what `adapter` publishes and its constants once (nothing for a Rust
    /// implementation): what `start()` does when it made a TS implementation.
    #[doc(hidden)]
    fn link(adapter: &Self::Dyn) -> Result<(), String>;
}

/// Contracts in an app's set: `registry.contract::<dyn WeatherSource>()` in
/// the body of the app's `#[rayx::native_module]` function adds `WeatherSource`'s interface and
/// its token function; `registry.contract_class::<dyn KeyValueStore, KeyValueStore>()` adds a
/// contract whose token is a RayX TS class.
pub trait ContractRegistryExt {
    /// Adds contract `C`'s TS interface and token function.
    fn contract<C: ?Sized + Contract>(&mut self) -> &mut Self;

    /// Adds contract `C`'s TS interface and the class `K`, which is its token: TS registers an
    /// implementation of `C` under `K` (`addSingletonAs(K, () => new MyImpl())`, and every other
    /// form), which Rust resolves as `dyn C`, and `getRequiredService(K)` gives a `K` over
    /// whichever `dyn C` is registered, written in Rust or in TS. `C` gets no token function.
    /// `K` declares `get contract(): <C's interface> | null` (TS types only) so the
    /// registrations' factories are typed by the interface.
    fn contract_class<C, K>(&mut self) -> &mut Self
    where
        C: ?Sized + Contract,
        K: ServiceClass<Service = C::Dyn>;
}

impl ContractRegistryExt for Registry {
    fn contract<C: ?Sized + Contract>(&mut self) -> &mut Self {
        C::register(self);
        self
    }

    fn contract_class<C, K>(&mut self) -> &mut Self
    where
        C: ?Sized + Contract,
        K: ServiceClass<Service = C::Dyn>,
    {
        C::register_interface(self);
        crate::di::provider_ts::register_class::<K>(Some(ContractDef::of::<C>()));
        self.class::<K>()
    }
}

/// Makes a contract's adapter in a provider and links it ([`ContractDef::made`]).
pub(crate) type MakeAdapter = fn(&rayx_app::ServiceProvider) -> Result<(), String>;

/// What a contract's token tells RayX's registrations.
#[derive(Clone, Copy)]
pub(crate) struct ContractDef {
    /// The startup key Rust services depending on `dyn Trait` name.
    pub(crate) startup: fn() -> StartupServiceType,
    /// Registers the adapter as `dyn Trait` over the TS service, with the lifetime.
    pub(crate) add: fn(&mut rayx_app::ServiceProvider, ServiceLifetime, SharedTsService),
    /// [`add`](Self::add), in a scope's own registrations (`createScope(configure)`).
    pub(crate) add_in_scope:
        fn(&mut rayx_app::di::ScopeRegistrations, ServiceLifetime, SharedTsService),
    /// Makes the adapter in a provider (on the engine thread, the TS service made) and links
    /// what it publishes and its constants; why it could not.
    pub(crate) made: MakeAdapter,
}

impl ContractDef {
    pub(crate) fn of<C: ?Sized + Contract>() -> Self {
        Self {
            startup: StartupServiceType::of::<C::Dyn>,
            add: |provider, lifetime, service| {
                add_adapter::<C>(provider, lifetime, service, false);
            },
            add_in_scope: |scope, lifetime, service| {
                add_adapter::<C>(scope, lifetime, service, true);
            },
            made: |provider| {
                provider
                    .service::<C::Dyn>()
                    .map_or(Ok(()), |adapter| C::link(&adapter))
            },
        }
    }
}

/// Why an adapter could not reach its TS object: it was made, or asked, off the engine thread.
const OFF_THREAD: &str = "its TS object is reached on the engine thread only";

/// Registers `C`'s adapter over the TS service `service`.
///
/// A singleton's and a scoped service's adapter look the TS object up on each call (a
/// singleton's in its registration, which keeps it once made; a scoped one's in the scope the
/// adapter was made in, kept weakly), so an adapter resolved before `start()` made the object,
/// by a Rust startup service's factory say, is never left without it. A transient adapter is
/// made over a new object, which it resolves on the engine thread when it is first called.
///
/// In a scope's own registrations (`in_scope`), a singleton's adapter looks its object up in
/// the scope that declared it, as a scoped one's does: no `start()` makes it, so it is made
/// there on the spot when first needed.
fn add_adapter<C: ?Sized + Contract>(
    registrations: &mut impl AdapterRegistrations,
    lifetime: ServiceLifetime,
    service: SharedTsService,
    in_scope: bool,
) {
    // The TS service's key, read here on the engine thread (a registration runs in a call from
    // JS), so an adapter made on another thread still knows what to look up on the engine.
    let key = service
        .get()
        .map(|service| service.key.clone())
        .map_err(|_| ContractError::Unavailable {
            contract: C::NAME,
            message: OFF_THREAD.into(),
        });
    let source = move |services: &rayx_app::ServiceProvider| {
        let runner = services
            .service::<EngineRunner>()
            .map(|runner| (*runner).clone());
        __private::ContractSource(
            runner,
            match lifetime {
                ServiceLifetime::Singleton if !in_scope => {
                    __private::Source::Singleton(Arc::clone(&service))
                }
                ServiceLifetime::Singleton | ServiceLifetime::Scoped => {
                    __private::Source::Scoped(Box::new(services.downgrade()), key.clone())
                }
                ServiceLifetime::Transient => __private::Source::Transient(
                    Box::new(services.downgrade()),
                    Arc::clone(&service),
                ),
            },
        )
    };
    let factory = move |services: &rayx_app::ServiceProvider| C::adapter(source(services));
    registrations.add_as::<C::Dyn>(lifetime, factory);
}

/// Where a contract's adapter is registered: the root's provider or a scope's own
/// registrations.
trait AdapterRegistrations {
    /// Registers `factory`'s `T` with `lifetime`.
    fn add_as<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&rayx_app::ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    );
}

impl AdapterRegistrations for rayx_app::ServiceProvider {
    fn add_as<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&rayx_app::ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) {
        match lifetime {
            ServiceLifetime::Singleton => self.replace_service_factory_as::<T>(factory),
            ServiceLifetime::Scoped => {
                self.add_scoped_as::<T>(factory);
            }
            ServiceLifetime::Transient => {
                self.add_transient_as::<T>(factory);
            }
        }
    }
}

impl AdapterRegistrations for rayx_app::di::ScopeRegistrations {
    fn add_as<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&rayx_app::ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) {
        match lifetime {
            ServiceLifetime::Singleton => self.add_singleton_factory_as::<T>(factory),
            ServiceLifetime::Scoped => self.add_scoped_as::<T>(factory),
            ServiceLifetime::Transient => self.add_transient_as::<T>(factory),
        };
    }
}

/// The TS object of the service under `key` in `services`, for contract `contract`.
fn resolved(
    services: &rayx_app::ServiceProvider,
    key: &RuntimeKey,
    contract: &'static str,
) -> Result<JsHandle, ContractError> {
    services
        .required_thread_bound_service_by_key(key)
        .map_err(|error| match error {
            ServiceResolutionError::WrongThread { .. } => ContractError::Unavailable {
                contract,
                message: OFF_THREAD.into(),
            },
            other => ContractError::Unavailable {
                contract,
                message: other.to_string(),
            },
        })
        .and_then(|value| {
            crate::di::container_ts::object(value, key)
                .map_err(|message| ContractError::Unavailable { contract, message })
        })
}

/// The VM's contract tokens: one `ServiceToken` per contract.
#[derive(Default)]
struct ContractTokens(RefCell<HashMap<TypeId, Instance<ServiceToken>>>);

impl Local for ContractTokens {
    fn close(&self) {
        self.0.borrow_mut().clear();
    }
}

/// Contract `C`'s token in the VM `sys` is of: the same one every time (what the token function
/// `#[contract]` exports gives).
pub fn contract_token<C: ?Sized + Contract>(sys: &System) -> Instance<ServiceToken> {
    let tokens = sys.locals.get::<ContractTokens>();
    let mut tokens = tokens.0.borrow_mut();
    tokens
        .entry(TypeId::of::<C>())
        .or_insert_with(|| {
            Instance::new(ServiceToken::contract(
                C::NAME.to_owned(),
                ContractDef::of::<C>(),
            ))
        })
        .clone()
}

/// Converts what TS publishes into one adapter's cell or stream: the value, or why it does not
/// convert. The returned function publishes it.
type Sink = Box<dyn Fn(&JsonValue) -> Result<Box<dyn FnOnce()>, String>>;

/// Converts `value` with every sink, then publishes it with each, or publishes nothing and says
/// why one refused it.
fn publish_to(sinks: &RefCell<Vec<Sink>>, value: &JsonValue) -> Result<(), String> {
    let commits = sinks
        .borrow()
        .iter()
        .map(|sink| sink(value))
        .collect::<Result<Vec<_>, _>>()?;
    for commit in commits {
        commit();
    }
    Ok(())
}

/// A contract's published state, as its TS implementation keeps it: `new ContractState(initial)`
/// for a `#[state]` member, then `.publish(value)` for each change. Rust reads the latest value
/// synchronously, from any thread, through the contract's `StateCell`.
///
/// ```ts
/// class MyTheme implements Theme {
///   readonly current = new ContractState("light");
///   toggle() { this.current.publish(this.current.value === "light" ? "dark" : "light"); }
/// }
/// ```
#[class(generic = "T")]
pub struct ContractState {
    value: RefCell<JsonValue>,
    sinks: RefCell<Vec<Sink>>,
}

#[class]
impl ContractState {
    /// State at `initial`.
    #[ts("(initial: T)")]
    pub fn new(initial: ContractJson) -> Self {
        Self {
            value: RefCell::new(initial.0),
            sinks: RefCell::default(),
        }
    }

    /// Replaces the value with `value` and tells Rust; throws, keeping the value, when it is not
    /// what the contract's state holds.
    #[ts("(value: T)")]
    pub fn publish(&self, value: ContractJson) -> Result<(), String> {
        publish_to(&self.sinks, &value.0)?;
        *self.value.borrow_mut() = value.0;
        Ok(())
    }

    /// The value now.
    #[getter]
    #[ts("(): T")]
    pub fn value(&self) -> JsonValue {
        self.value.borrow().clone()
    }
}

/// A contract's published events, as its TS implementation sends them: `new ContractEvents()`
/// for an `#[events]` member, then `.publish(event)` for each. Rust subscribers hear each one
/// once, on any thread; nothing is kept.
#[class(generic = "E")]
pub struct ContractEvents {
    sinks: RefCell<Vec<Sink>>,
}

#[class]
impl ContractEvents {
    /// Events nobody heard yet.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            sinks: RefCell::default(),
        }
    }

    /// Tells Rust about `event`; throws, telling nobody, when it is not what the contract's
    /// events are.
    #[ts("(event: E)")]
    pub fn publish(&self, event: ContractJson) -> Result<(), String> {
        publish_to(&self.sinks, &event.0)
    }
}

/// What a TS implementation of a core contract gives Rust, as JSON: what `JSON.stringify`
/// makes of it, except bytes, which cross as they are wherever they are (a result, a field, an
/// item): a `Uint8Array` (any typed array, a `DataView`) or an `ArrayBuffer` is the array of its
/// byte values, which a `Vec<u8>` takes, as it takes a `number[]`.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractJson(pub JsonValue);

impl nsh_core::element::JsValue for ContractJson {
    fn ts_type(_types: &mut nsh_core::element::TsTypes) -> String {
        "unknown".into()
    }

    fn from_js(value: nsh_core::element::JsIn<'_>) -> Result<Self, nsh_core::element::PropError> {
        json_of(value, 0).map(Self)
    }

    fn to_js<'a>(
        &self,
        host: &'a dyn nsh_core::element::JsHost,
    ) -> Result<nsh_core::element::JsOwned<'a>, nsh_core::element::PropError> {
        self.0.to_js(host)
    }
}

/// How deep [`json_of`] walks before it leaves the rest to `JSON.stringify` (a cycle throws
/// there, as it would).
const MAX_JSON_DEPTH: usize = 64;

/// `value` as JSON, bytes as arrays of their values.
fn json_of(
    value: nsh_core::element::JsIn<'_>,
    depth: usize,
) -> Result<JsonValue, nsh_core::element::PropError> {
    use nsh_core::element::{JsClass, JsKind, JsPrimitive};
    if depth > MAX_JSON_DEPTH {
        return value.to_json();
    }
    match value.class() {
        Some(JsClass::ArrayBuffer | JsClass::View(_)) => {
            if let Some(bytes) = value.as_buffer() {
                return Ok(JsonValue::from(bytes));
            }
        }
        Some(JsClass::Array) => {
            let Some(items) = value.primitives()? else {
                return value.to_json();
            };
            let mut array = Vec::with_capacity(items.len());
            for (index, item) in items.into_iter().enumerate() {
                array.push(match item {
                    JsPrimitive::Bool(value) => JsonValue::Bool(value),
                    JsPrimitive::Number(number) => json_number(number),
                    JsPrimitive::String(value) => JsonValue::String(value),
                    // `undefined` in an array is `null`, as `JSON.stringify` has it.
                    JsPrimitive::Date(_) | JsPrimitive::Other => {
                        let item = value.index(index)?;
                        match item.get().kind() {
                            JsKind::Undefined | JsKind::Function => JsonValue::Null,
                            _ => json_of(item.get(), depth + 1)?,
                        }
                    }
                });
            }
            return Ok(JsonValue::Array(array));
        }
        Some(JsClass::Object) => {
            // An object that says how it is JSON (`toJSON`) is left to `JSON.stringify`.
            if value.field("toJSON")?.get().kind() == JsKind::Function {
                return value.to_json();
            }
            let mut object = serde_json::Map::new();
            for key in value.keys()? {
                let field = value.field(&key)?;
                // Skipped by `JSON.stringify`.
                if matches!(field.get().kind(), JsKind::Undefined | JsKind::Function) {
                    continue;
                }
                object.insert(key, json_of(field.get(), depth + 1)?);
            }
            return Ok(JsonValue::Object(object));
        }
        _ => {}
    }
    match value.kind() {
        JsKind::Undefined => Ok(JsonValue::Null),
        _ => value.to_json(),
    }
}

/// A JS number as JSON: whole numbers as integers (what a Rust integer takes), `NaN` and the
/// infinities as `null`, as `JSON.stringify` has them.
fn json_number(number: f64) -> JsonValue {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    if number.is_finite() && number.fract() == 0.0 && number.abs() <= MAX_SAFE {
        // In range, and whole: exact.
        #[allow(clippy::cast_possible_truncation)]
        JsonValue::from(number as i64)
    } else {
        serde_json::Number::from_f64(number).map_or(JsonValue::Null, JsonValue::Number)
    }
}

/// What `#[contract]`'s code names.
#[doc(hidden)]
pub mod __private {
    pub use futures::future::LocalBoxFuture;

    use std::sync::{Mutex, PoisonError};

    use futures::FutureExt as _;

    use super::*;

    /// What a contract's adapter calls: a singleton's registration (its object looked up on
    /// each call), or the object of the scope that made the adapter (or why there is none), and
    /// the surface's runner, which takes calls from other threads to the engine.
    pub struct ContractSource(pub(crate) Option<EngineRunner>, pub(crate) Source);

    pub(crate) enum Source {
        Singleton(SharedTsService),
        /// The scope the adapter was made in, and the TS service's key there.
        Scoped(Box<WeakServiceProvider>, Result<RuntimeKey, ContractError>),
        /// The scope the adapter was made in, and the TS service it makes a new object of, on
        /// the engine thread, when first called.
        Transient(Box<WeakServiceProvider>, SharedTsService),
    }

    /// A posted member's call, waiting for its transient object.
    type Post<S> = Box<dyn FnOnce(Rc<S>) + Send>;

    /// A transient adapter's object being made: shared by every call that awaits it.
    type Making = futures::future::Shared<LocalBoxFuture<'static, Result<JsHandle, String>>>;

    /// A contract adapter's stand-in, kept on the engine thread, and the way there from other
    /// threads: `Send + Sync`, and its clones share it.
    pub struct ContractSlot<S: 'static>(Arc<Slot<S>>);

    struct Slot<S: 'static> {
        contract: &'static str,
        runner: Option<EngineRunner>,
        object: Object<S>,
    }

    impl<S: 'static> Clone for ContractSlot<S> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }

    enum Object<S: 'static> {
        Singleton(SharedTsService, fn(JsHandle) -> S),
        Scoped(
            Box<WeakServiceProvider>,
            Result<RuntimeKey, ContractError>,
            fn(JsHandle) -> S,
        ),
        Transient {
            scope: Box<WeakServiceProvider>,
            service: SharedTsService,
            stand_in: fn(JsHandle) -> S,
            made: OnceLock<ThreadBound<S>>,
            /// The make in flight, which every call before it ends awaits (TS's factory runs
            /// once).
            making: Mutex<Option<ThreadBound<Making>>>,
            /// Posted members called before the object was made, run in order once it is.
            posts: Mutex<Vec<Post<S>>>,
        },
    }

    impl<S: 'static> ContractSlot<S> {
        /// The stand-in (`stand_in` makes it from the JS object) of what `source` gives.
        pub fn new(
            source: ContractSource,
            contract: &'static str,
            stand_in: fn(JsHandle) -> S,
        ) -> Self {
            let ContractSource(runner, source) = source;
            let object = match source {
                Source::Singleton(service) => Object::Singleton(service, stand_in),
                Source::Scoped(scope, key) => Object::Scoped(scope, key, stand_in),
                Source::Transient(scope, service) => Object::Transient {
                    scope,
                    service,
                    stand_in,
                    made: OnceLock::new(),
                    making: Mutex::new(None),
                    posts: Mutex::new(Vec::new()),
                },
            };
            Self(Arc::new(Slot {
                contract,
                runner,
                object,
            }))
        }

        /// The contract's name.
        pub fn contract(&self) -> &'static str {
            self.0.contract
        }

        /// Whether this is the engine thread, where the adapter links at once.
        pub fn is_engine_thread(&self) -> bool {
            self.on_engine_thread()
        }

        /// Whether this is the engine thread: the thread the stand-in lives on.
        fn on_engine_thread(&self) -> bool {
            match &self.0.runner {
                Some(runner) => runner.is_engine_thread(),
                // No runner (no app start ran a dispatcher): the stand-in is reached where it
                // is, or not at all.
                None => true,
            }
        }

        /// The stand-in, on the engine thread.
        pub fn get(&self) -> Result<Rc<S>, ContractError> {
            let contract = self.0.contract;
            let off_thread = |_| ContractError::Unavailable {
                contract,
                message: OFF_THREAD.into(),
            };
            match &self.0.object {
                Object::Singleton(service, stand_in) => {
                    let service = service.get().map_err(off_thread)?;
                    let object = service
                        .singleton()
                        .ok_or_else(|| ContractError::Unavailable {
                            contract,
                            message: "its TS singleton is not made yet: `start()` makes it".into(),
                        })?;
                    Ok(Rc::new(stand_in(object)))
                }
                Object::Scoped(scope, key, stand_in) => {
                    let key = key.as_ref().map_err(Clone::clone)?;
                    let scope = scope.upgrade().ok_or_else(|| ContractError::Unavailable {
                        contract,
                        message: "its scope is gone".into(),
                    })?;
                    let object = super::resolved(&scope, key, contract)?;
                    Ok(Rc::new(stand_in(object)))
                }
                Object::Transient {
                    scope,
                    service,
                    stand_in,
                    made,
                    ..
                } => {
                    if let Some(made) = made.get() {
                        return made.get().map_err(off_thread);
                    }
                    // Made here when TS can make it now (inside a call from JS); else
                    // `get_async` makes it.
                    let key = service.get().map_err(off_thread)?.key.clone();
                    let scope = scope.upgrade().ok_or_else(|| ContractError::Unavailable {
                        contract,
                        message: "its scope is gone".into(),
                    })?;
                    let object = super::resolved(&scope, &key, contract)?;
                    self.keep_made(made, stand_in(object))
                }
            }
        }

        /// The stand-in, on the engine thread, made first when it is a transient one TS has not
        /// made yet (its factory called in JS's next turn).
        pub async fn get_async(&self) -> Result<Rc<S>, ContractError> {
            let contract = self.0.contract;
            let Object::Transient {
                scope,
                service,
                stand_in,
                made,
                making,
                ..
            } = &self.0.object
            else {
                return self.get();
            };
            if let Ok(object) = self.get() {
                self.run_posts(&object);
                return Ok(object);
            }
            let unavailable = |message: String| ContractError::Unavailable { contract, message };
            // One make for every call that comes before it ends.
            let shared = {
                let mut making = making.lock().unwrap_or_else(PoisonError::into_inner);
                let kept = making
                    .as_ref()
                    .and_then(|kept| kept.get().ok())
                    .map(|kept| (*kept).clone());
                match kept {
                    Some(shared) => shared,
                    None => {
                        let registration =
                            service.get().map_err(|_| unavailable(OFF_THREAD.into()))?;
                        let scope = scope
                            .upgrade()
                            .ok_or_else(|| unavailable("its scope is gone".into()))?;
                        let shared: Making = async move {
                            crate::di::container_ts::make(&scope, &registration).await
                        }
                        .boxed_local()
                        .shared();
                        *making = ThreadBound::new(shared.clone(), contract).ok();
                        shared
                    }
                }
            };
            let object = shared.await.map_err(unavailable)?;
            let object = match made.get() {
                Some(made) => made.get().map_err(|_| unavailable(OFF_THREAD.into()))?,
                None => self.keep_made(made, stand_in(object))?,
            };
            self.run_posts(&object);
            Ok(object)
        }

        /// Runs the posted members that came before the object was made, in order.
        fn run_posts(&self, object: &Rc<S>) {
            if let Object::Transient { posts, .. } = &self.0.object {
                let waiting =
                    std::mem::take(&mut *posts.lock().unwrap_or_else(PoisonError::into_inner));
                for post in waiting {
                    post(Rc::clone(object));
                }
            }
        }

        /// Keeps `post` for when the transient object is made (true), or says it is made already
        /// (false).
        fn wait_to_post(&self, post: Post<S>) -> Result<(), Post<S>> {
            let Object::Transient { made, posts, .. } = &self.0.object else {
                return Err(post);
            };
            // Checked and queued under the queue's lock, which `run_posts` takes after the object
            // is kept: a post either sees it made or is drained with the others.
            let mut posts = posts.lock().unwrap_or_else(PoisonError::into_inner);
            if made.get().is_some() {
                return Err(post);
            }
            posts.push(post);
            Ok(())
        }

        /// Whether the stand-in is a transient one, made on the engine when first needed.
        fn makes_later(&self) -> bool {
            matches!(self.0.object, Object::Transient { .. })
        }

        /// Keeps `stand_in`, a transient adapter's object, and gives it (the one kept first, when
        /// two were made).
        fn keep_made(
            &self,
            made: &OnceLock<ThreadBound<S>>,
            stand_in: S,
        ) -> Result<Rc<S>, ContractError> {
            let contract = self.0.contract;
            let bound = ThreadBound::new(stand_in, contract).map_err(|error| {
                ContractError::Unavailable {
                    contract,
                    message: error.to_string(),
                }
            })?;
            made.get_or_init(|| bound)
                .get()
                .map_err(|_| ContractError::Unavailable {
                    contract,
                    message: OFF_THREAD.into(),
                })
        }
    }

    /// Calls a command of the TS implementation behind `slot` with what `call` makes of its
    /// stand-in: as an engine task at once on the engine thread (in a task or a component),
    /// through the surface's queue from any other thread or a synchronous call from JS. Resolves with its result, or `Err(ContractError::EngineGone)` once the
    /// engine stopped.
    pub fn call<S, F, Fut, T, E>(slot: ContractSlot<S>, call: F) -> BoxFuture<'static, Result<T, E>>
    where
        S: 'static,
        F: FnOnce(Rc<S>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        T: Send + 'static,
        E: From<ContractError> + Send + 'static,
    {
        let contract = slot.contract();
        let Some(runner) = slot.0.runner.clone() else {
            // No app start ran the surface's dispatcher: the stand-in is reached on this thread,
            // or not at all.
            if !slot.on_engine_thread() {
                return failed(ContractError::EngineGone { contract });
            }
            return spawn_call(contract, async move {
                match slot.get_async().await {
                    Ok(object) => call(object).await,
                    Err(error) => Err(E::from(error)),
                }
            });
        };
        // At once on the engine thread while it runs Rust code, through the queue otherwise.
        let ran = runner.run(move || async move {
            match slot.get_async().await {
                Ok(object) => call(object).await,
                Err(error) => Err(E::from(error)),
            }
        });
        Box::pin(async move {
            ran.await
                .unwrap_or_else(|_| Err(E::from(ContractError::EngineGone { contract })))
        })
    }

    /// Calls a posted member of the TS implementation behind `slot` with `call`: on the engine
    /// thread at once (on the spot inside a call from JS, else posted to JS's next turn), queued
    /// for the engine from any other thread; dropped, with a line saying so, once the engine
    /// stopped.
    pub fn post<S, F>(slot: ContractSlot<S>, member: &'static str, call: F)
    where
        S: 'static,
        F: FnOnce(Rc<S>) + Send + 'static,
    {
        let contract = slot.contract();
        if slot.on_engine_thread() {
            match slot.get() {
                Ok(object) => return call(object),
                // A transient object TS has not made yet: made on the engine, then called.
                Err(_) if slot.makes_later() => {}
                Err(error) => return dropped(error, member),
            }
        }
        let Some(runner) = slot.0.runner.clone() else {
            return dropped(ContractError::EngineGone { contract }, member);
        };
        if slot.makes_later() {
            // Kept in order with the other early posts, run once the object is made.
            let call: Post<S> = Box::new(call);
            let Err(call) = slot.wait_to_post(call) else {
                let ran = runner.run(move || async move {
                    if let Err(error) = slot.get_async().await {
                        dropped(error, member);
                    }
                });
                // The work is queued (or running) already: nothing waits for it.
                drop(ran);
                return;
            };
            let posted = runner.post(move || match slot.get() {
                Ok(object) => call(object),
                Err(error) => dropped(error, member),
            });
            if posted.is_err() {
                dropped(ContractError::EngineGone { contract }, member);
            }
            return;
        }
        let posted = runner.post(move || match slot.get() {
            Ok(object) => call(object),
            Err(error) => dropped(error, member),
        });
        if posted.is_err() {
            dropped(ContractError::EngineGone { contract }, member);
        }
    }

    /// A method returning nothing that could not be posted: dropped, with a line saying so.
    pub fn dropped(error: ContractError, method: &str) {
        eprintln!("rayx: a call of `{method}` was dropped: {error}");
    }

    pub use futures::future::BoxFuture;

    pub use super::ContractJson;

    /// A value of a core contract's member as JSON (`null` when it does not serialize).
    pub fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> JsonValue {
        serde_json::to_value(value).unwrap_or(JsonValue::Null)
    }

    /// [`call`] for a core contract's command, whose values cross as JSON: the result converted
    /// to `T`, and every failure (what TS threw, a result that does not convert, the engine
    /// gone) as the core's `ContractFailure`, which the service's error type converts from.
    pub fn call_json<S, F, Fut, T, E>(
        slot: ContractSlot<S>,
        member: &'static str,
        call: F,
    ) -> BoxFuture<'static, Result<T, E>>
    where
        S: 'static,
        F: FnOnce(Rc<S>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<JsonValue, String>> + 'static,
        T: serde::de::DeserializeOwned + Send + 'static,
        E: From<rayx_app::services::ContractFailure> + Send + 'static,
    {
        let contract = slot.contract();
        let failure = move |message: String| rayx_app::services::ContractFailure {
            contract,
            member,
            message,
        };
        let answered = super::__private::call::<S, _, _, JsonValue, Failed>(slot, move |object| {
            let answer = call(object);
            async move { answer.await.map_err(Failed::Threw) }
        });
        Box::pin(async move {
            match answered.await {
                Ok(value) => serde_json::from_value(value).map_err(|error| {
                    E::from(failure(format!("its result does not convert: {error}")))
                }),
                Err(Failed::Threw(message)) => Err(E::from(failure(message))),
                Err(Failed::Contract(error)) => Err(E::from(failure(error.to_string()))),
            }
        })
    }

    /// Why a JSON call failed, before it is the service's error.
    pub enum Failed {
        /// What TS threw.
        Threw(String),
        /// The call did not reach TS.
        Contract(ContractError),
    }

    impl From<ContractError> for Failed {
        fn from(error: ContractError) -> Self {
            Self::Contract(error)
        }
    }

    /// Runs `call` (a call of the TS implementation, made on the engine thread) as a task of the
    /// engine and gives a `Send` future of its result.
    pub fn spawn_call<T, E>(
        contract: &'static str,
        call: impl Future<Output = Result<T, E>> + 'static,
    ) -> BoxFuture<'static, Result<T, E>>
    where
        T: Send + 'static,
        E: From<ContractError> + Send + 'static,
    {
        let (send, receive) = futures::channel::oneshot::channel();
        // Spawned here where NSH's runtime is entered, started as the call's op inside a
        // synchronous call from JS.
        let started = start_task(async move {
            let _ = send.send(call.await);
        });
        if started.is_err() {
            return failed(ContractError::EngineGone { contract });
        }
        Box::pin(async move {
            receive
                .await
                .unwrap_or_else(|_| Err(E::from(ContractError::EngineGone { contract })))
        })
    }

    /// A command that could not reach its TS implementation.
    pub fn failed<T, E>(error: ContractError) -> BoxFuture<'static, Result<T, E>>
    where
        T: Send + 'static,
        E: From<ContractError> + Send + 'static,
    {
        Box::pin(futures::future::ready(Err(E::from(error))))
    }

    /// Runs `link` (an adapter's linking, which reads its TS object) on the engine thread: here
    /// when this is it, else through the surface's runner, waiting for it (a thread that is not
    /// the engine's may wait; never block the engine thread on that thread meanwhile). A first
    /// read off the engine thread waits here, so it never runs in a factory off the engine
    /// thread, where the container holds the made service's lock the engine thread may wait on.
    pub fn link_on_engine<S: 'static>(
        slot: &ContractSlot<S>,
        link: impl FnOnce() -> Result<(), ContractError> + Send + 'static,
    ) -> Result<(), ContractError> {
        let contract = slot.contract();
        if slot.on_engine_thread() {
            return link();
        }
        let Some(runner) = slot.0.runner.clone() else {
            return Err(ContractError::EngineGone { contract });
        };
        let (send, receive) = std::sync::mpsc::channel();
        runner
            .post(move || {
                let _ = send.send(link());
            })
            .map_err(|_| ContractError::EngineGone { contract })?;
        receive
            .recv()
            .unwrap_or(Err(ContractError::EngineGone { contract }))
    }

    /// What an adapter linked once, on the engine thread, or why it could not: a member that is
    /// missing or does not convert, kept so the adapter never reads JS for it again.
    pub struct ContractLinked<L>(OnceLock<Result<L, ContractError>>);

    impl<L> Default for ContractLinked<L> {
        fn default() -> Self {
            Self(OnceLock::new())
        }
    }

    impl<L> ContractLinked<L> {
        /// What was linked, or why it failed, once either happened.
        pub fn get(&self) -> Option<&Result<L, ContractError>> {
            self.0.get()
        }

        /// Keeps `linked` (the first one, when two links raced), or keeps its failure when a
        /// member was wrong; another failure (no object yet) is not kept. What is kept, as a
        /// result.
        pub fn set(&self, linked: Result<L, ContractError>) -> Result<(), ContractError> {
            match linked {
                Err(error) if !matches!(error, ContractError::Member { .. }) => Err(error),
                linked => self
                    .0
                    .get_or_init(|| linked)
                    .as_ref()
                    .map(|_| ())
                    .map_err(Clone::clone),
            }
        }
    }

    /// What linking an adapter when it was made gave: a member that is missing or does not
    /// convert is reported now (the reads panic with it later); an object not made yet (a
    /// singleton before `start()`, which links it) is not.
    pub fn linked_when_made(linked: Result<(), ContractError>, contract: &str) {
        if let Err(error @ ContractError::Member { .. }) = linked {
            eprintln!("rayx: `{contract}`'s TS implementation could not be linked: {error}");
        }
    }

    /// What `linked` gives; panics, naming `contract` and `member`, when the adapter could not
    /// link: a member of the TS implementation that is missing or does not convert, or a read
    /// where it cannot be linked (a singleton read before `start()` made it, the engine gone).
    pub fn linked<'a, L>(
        linked: Result<&'a L, ContractError>,
        contract: &str,
        member: &str,
    ) -> &'a L {
        linked.unwrap_or_else(|error| match error {
            ContractError::Member { .. } => panic!(
                "`{contract}.{member}`: RayX could not link the contract's TS implementation: \
                 {error}"
            ),
            error => panic!(
                "`{contract}.{member}` is read where RayX cannot link the contract's TS \
                 implementation: {error}"
            ),
        })
    }

    fn member_error(
        contract: &'static str,
        member: &'static str,
        message: String,
    ) -> ContractError {
        ContractError::Member {
            contract,
            member,
            message,
        }
    }

    /// The cell of a `#[state]` member over the `ContractState` the TS implementation gave
    /// (`got`): at its value now, then every value TS publishes.
    pub fn link_state<T>(
        got: Result<Instance<ContractState>, String>,
        contract: &'static str,
        member: &'static str,
    ) -> Result<StateCell<T>, ContractError>
    where
        T: serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        let state = got.map_err(|message| member_error(contract, member, message))?;
        let initial = state.with(|state| state.value.borrow().clone());
        let initial: T = serde_json::from_value(initial).map_err(|error| {
            member_error(
                contract,
                member,
                format!("its value does not convert: {error}"),
            )
        })?;
        let cell = StateCell::new(initial);
        let target = cell.clone();
        let sink: Sink = Box::new(move |value| {
            let value: T = serde_json::from_value(value.clone()).map_err(|error| {
                format!("`{contract}.{member}`: the value does not convert: {error}")
            })?;
            let target = target.clone();
            Ok(Box::new(move || target.publish(value)) as Box<dyn FnOnce()>)
        });
        state.with(|state| state.sinks.borrow_mut().push(sink));
        Ok(cell)
    }

    /// The stream of an `#[events]` member over the `ContractEvents` the TS implementation gave
    /// (`got`): every event TS publishes from now on.
    pub fn link_events<E>(
        got: Result<Instance<ContractEvents>, String>,
        contract: &'static str,
        member: &'static str,
    ) -> Result<EventStream<E>, ContractError>
    where
        E: serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        let events = got.map_err(|message| member_error(contract, member, message))?;
        let stream = EventStream::new();
        let target = stream.clone();
        let sink: Sink = Box::new(move |event| {
            let event: E = serde_json::from_value(event.clone()).map_err(|error| {
                format!("`{contract}.{member}`: the event does not convert: {error}")
            })?;
            let target = target.clone();
            Ok(Box::new(move || target.publish(event)) as Box<dyn FnOnce()>)
        });
        events.with(|events| events.sinks.borrow_mut().push(sink));
        Ok(stream)
    }

    /// A `#[constant]` member's value, read once from what the TS implementation gave (`got`).
    pub fn constant<T>(
        got: Result<JsonValue, String>,
        contract: &'static str,
        member: &'static str,
    ) -> Result<T, ContractError>
    where
        T: serde::de::DeserializeOwned,
    {
        let value = got.map_err(|message| member_error(contract, member, message))?;
        serde_json::from_value(value).map_err(|error| {
            member_error(
                contract,
                member,
                format!("the constant is missing or does not convert: {error}"),
            )
        })
    }
}
