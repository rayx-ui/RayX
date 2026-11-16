//! Contracts: Rust traits TS implements (`#[contract]`), which Rust resolves as `Arc<dyn Trait>`.
//!
//! `#[contract]` keeps the trait `dyn`-compatible for Rust and makes, beside it, the NSH
//! interface of the trait's name (its stand-in `Js<Trait>` calls the JS object), the token TS
//! registers an implementation under (a flat function `<trait>Token()` giving the contract's
//! `ServiceToken`) and an adapter: `Send + Sync`, it implements the trait over the stand-in kept
//! on the engine thread ([`ContractSlot`]). Registering a TS implementation under the token
//! (`addSingletonAs(weatherSourceToken(), () => new OpenMeteo())`, and the other forms) also
//! registers the adapter as `dyn Trait`, with the same lifetime; a singleton (and every startup
//! form) is made during `start()`, so Rust resolves it synchronously from then on, and Rust
//! startup services depending on `dyn Trait` initialize after it.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use nsh_core::element::{Local, Registry};
use nsh_core::prelude::*;
use rayx_core::di::{
    RuntimeKey, ServiceLifetime, ServiceResolutionError, StartupServiceType, ThreadBound,
    WeakServiceProvider,
};

use super::container::SharedTsService;
use super::provider::ServiceToken;

pub use nsh_core::element::Exception;

/// Why a contract's call did not reach its TS implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractError {
    /// Called off the engine thread: TS objects are reached on it only.
    WrongThread { contract: &'static str },
    /// The TS implementation could not be resolved when the adapter was made (`message` says
    /// why).
    Unavailable {
        contract: &'static str,
        message: String,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongThread { contract } => write!(
                f,
                "`{contract}` is implemented in TS: call it on the engine thread"
            ),
            Self::Unavailable { contract, message } => {
                write!(
                    f,
                    "`{contract}` has no TS implementation to call: {message}"
                )
            }
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
pub trait Contract: Send + Sync + 'static {
    /// The trait's name, which is its TS interface's.
    const NAME: &'static str;

    /// Adds the contract's interface and token to an app's set.
    #[doc(hidden)]
    fn register(registry: &mut Registry);

    /// The adapter over the TS implementation `source` gives.
    #[doc(hidden)]
    fn adapter(source: __private::ContractSource) -> Arc<Self>;
}

/// Contracts in an app's set: `registry.contract::<dyn WeatherSource>()` in
/// the body of the app's `#[rayx::native_module]` function adds `WeatherSource`'s interface and
/// its token function.
pub trait ContractRegistryExt {
    /// Adds contract `C`'s TS interface and token function.
    fn contract<C: ?Sized + Contract>(&mut self) -> &mut Self;
}

impl ContractRegistryExt for Registry {
    fn contract<C: ?Sized + Contract>(&mut self) -> &mut Self {
        C::register(self);
        self
    }
}

/// What a contract's token tells RayX's registrations.
#[derive(Clone, Copy)]
pub(crate) struct ContractDef {
    /// The startup key Rust services depending on `dyn Trait` name.
    pub(crate) startup: fn() -> StartupServiceType,
    /// Registers the adapter as `dyn Trait` over the TS service, with the lifetime.
    pub(crate) add: fn(&mut rayx_core::ServiceProvider, ServiceLifetime, SharedTsService),
    /// Makes the adapter in a provider (on the engine thread, the TS service made).
    pub(crate) made: fn(&rayx_core::ServiceProvider),
}

impl ContractDef {
    fn of<C: ?Sized + Contract>() -> Self {
        Self {
            startup: StartupServiceType::of::<C>,
            add: add_adapter::<C>,
            made: |provider| drop(provider.service::<C>()),
        }
    }
}

/// Registers `C`'s adapter over the TS service `service`.
///
/// A singleton's and a scoped service's adapter look the TS object up on each call (a
/// singleton's in its registration, which keeps it once made; a scoped one's in the scope the
/// adapter was made in, kept weakly), so an adapter resolved before `start()` made the object,
/// by a Rust startup service's factory say, is never left without it. A transient adapter is
/// made over a new object, which it resolves when it is made (in a call from JS).
fn add_adapter<C: ?Sized + Contract>(
    provider: &mut rayx_core::ServiceProvider,
    lifetime: ServiceLifetime,
    service: SharedTsService,
) {
    let source = move |services: &rayx_core::ServiceProvider| {
        __private::ContractSource(match lifetime {
            ServiceLifetime::Singleton => __private::Source::Singleton(Arc::clone(&service)),
            ServiceLifetime::Scoped => __private::Source::Scoped(
                Box::new(services.downgrade()),
                service
                    .get()
                    .map(|service| service.key.clone())
                    .map_err(|_| ContractError::WrongThread { contract: C::NAME }),
            ),
            ServiceLifetime::Transient => __private::Source::Made(
                service
                    .get()
                    .map_err(|_| ContractError::WrongThread { contract: C::NAME })
                    .and_then(|service| resolved(services, &service.key, C::NAME)),
            ),
        })
    };
    let factory = move |services: &rayx_core::ServiceProvider| C::adapter(source(services));
    match lifetime {
        ServiceLifetime::Singleton => provider.replace_service_factory_as::<C>(factory),
        ServiceLifetime::Scoped => {
            provider.add_scoped_as::<C>(factory);
        }
        ServiceLifetime::Transient => {
            provider.add_transient_as::<C>(factory);
        }
    }
}

/// The TS object of the service under `key` in `services`, for contract `contract`.
fn resolved(
    services: &rayx_core::ServiceProvider,
    key: &RuntimeKey,
    contract: &'static str,
) -> Result<JsHandle, ContractError> {
    services
        .required_thread_bound_service_by_key(key)
        .map_err(|error| match error {
            ServiceResolutionError::WrongThread { .. } => ContractError::WrongThread { contract },
            other => ContractError::Unavailable {
                contract,
                message: other.to_string(),
            },
        })
        .and_then(|value| {
            super::container::object(value, key)
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

/// What `#[contract]`'s code names.
#[doc(hidden)]
pub mod __private {
    pub use futures::future::LocalBoxFuture;

    use super::*;

    /// What a contract's adapter calls: a singleton's registration (its object looked up on
    /// each call), or the object of the scope that made the adapter (or why there is none).
    pub struct ContractSource(pub(crate) Source);

    pub(crate) enum Source {
        Singleton(SharedTsService),
        /// The scope the adapter was made in, and the TS service's key there.
        Scoped(Box<WeakServiceProvider>, Result<RuntimeKey, ContractError>),
        Made(Result<JsHandle, ContractError>),
    }

    /// A contract adapter's stand-in, kept on the engine thread: `Send + Sync`, it gives the
    /// stand-in there and [`ContractError::WrongThread`] anywhere else.
    pub struct ContractSlot<S: 'static> {
        contract: &'static str,
        object: Object<S>,
    }

    enum Object<S: 'static> {
        Singleton(SharedTsService, fn(JsHandle) -> S),
        Scoped(
            Box<WeakServiceProvider>,
            Result<RuntimeKey, ContractError>,
            fn(JsHandle) -> S,
        ),
        Made(Result<ThreadBound<S>, ContractError>),
    }

    impl<S: 'static> ContractSlot<S> {
        /// The stand-in (`stand_in` makes it from the JS object) of what `source` gives.
        pub fn new(
            source: ContractSource,
            contract: &'static str,
            stand_in: fn(JsHandle) -> S,
        ) -> Self {
            let object = match source.0 {
                Source::Singleton(service) => Object::Singleton(service, stand_in),
                Source::Scoped(scope, key) => Object::Scoped(scope, key, stand_in),
                Source::Made(object) => Object::Made(object.and_then(|object| {
                    ThreadBound::new(stand_in(object), contract).map_err(|error| {
                        ContractError::Unavailable {
                            contract,
                            message: error.to_string(),
                        }
                    })
                })),
            };
            Self { contract, object }
        }

        /// The stand-in, on the engine thread.
        pub fn get(&self) -> Result<Rc<S>, ContractError> {
            let wrong_thread = |_| ContractError::WrongThread {
                contract: self.contract,
            };
            match &self.object {
                Object::Singleton(service, stand_in) => {
                    let service = service.get().map_err(wrong_thread)?;
                    let object = service
                        .singleton()
                        .ok_or_else(|| ContractError::Unavailable {
                            contract: self.contract,
                            message: "its TS singleton is not made yet: `start()` makes it".into(),
                        })?;
                    Ok(Rc::new(stand_in(object)))
                }
                Object::Scoped(scope, key, stand_in) => {
                    let key = key.as_ref().map_err(Clone::clone)?;
                    let scope = scope.upgrade().ok_or_else(|| ContractError::Unavailable {
                        contract: self.contract,
                        message: "its scope is gone".into(),
                    })?;
                    let object = super::resolved(&scope, key, self.contract)?;
                    Ok(Rc::new(stand_in(object)))
                }
                Object::Made(object) => {
                    let object = object.as_ref().map_err(Clone::clone)?;
                    object.get().map_err(wrong_thread)
                }
            }
        }
    }

    /// A method returning nothing that could not be posted: dropped, with a line saying so.
    pub fn dropped(error: ContractError, method: &str) {
        eprintln!("rayx: a call of `{method}` was dropped: {error}");
    }
}
