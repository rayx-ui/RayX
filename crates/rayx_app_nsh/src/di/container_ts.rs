//! Services written in TS, in RayX's one container.
//!
//! A TS service is a thread-bound, runtime-keyed entry of the Rust provider: its value is the
//! JS object its factory made (a [`JsHandle`]), kept on the engine thread, and its key is the
//! one its token (a TS class, a `ServiceToken`) names in the VM's [`ServiceTokens`]. Lifetimes,
//! scopes and caching are the provider's own, so a singleton is one object per root, a scoped
//! one an object per scope (released with it) and a transient one new per resolve.
//!
//! Its factory is a JS function. Inside a synchronous call from JS (a TS `getService`, or
//! another TS factory) it runs on the spot ([`Callback::call`]); outside one (Rust code in a
//! task, `start()`'s startup run) it cannot ([`Callback::can_call_now`] says which), so a
//! service that is not made yet fails to resolve there, saying so. The startup run makes what Rust needs made with [`make`], which calls the
//! factory with [`Callback::call_async`] and hands the object to the provider's own factory.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nsh_core::prelude::*;
use rayx_app::di::{
    LocalService, RuntimeKey, ServiceLifetime, ServiceResolutionError, ThreadBound,
};

use crate::di::provider_ts::{ServiceKeyArg, ServiceProvider, provider_for, root_of};

/// A TS service's factory: the resolving `ServiceProvider` in, the service's JS object out.
pub(crate) type Factory = Callback<Instance<ServiceProvider>, JsHandle>;

/// A TS service's registration, kept on the engine thread.
pub(crate) struct TsService {
    /// Tells this registration's object apart while it is handed to the provider.
    id: u64,
    /// What it is registered under.
    pub(crate) key: RuntimeKey,
    factory: Factory,
    /// The VM it was registered from (its tokens and root object).
    pub(crate) sys: System,
    /// A singleton's object once made: what a contract's adapter calls.
    singleton: RefCell<Option<JsHandle>>,
}

/// A registration as the provider's `Send + Sync` factories hold it.
pub(crate) type SharedTsService = Arc<ThreadBound<TsService>>;

impl TsService {
    /// The singleton's object, once the provider made it (never for another lifetime).
    pub(crate) fn singleton(&self) -> Option<JsHandle> {
        self.singleton.borrow().clone()
    }

    /// The registration of a TS service under `token`'s key (made the first time).
    pub(crate) fn new(
        token: &ServiceKeyArg,
        factory: Factory,
        sys: &System,
    ) -> Result<SharedTsService, String> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let key = token.key_or_insert(sys);
        let name = key.name().to_owned();
        let service = Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            key,
            factory,
            sys: sys.clone(),
            singleton: RefCell::new(None),
        };
        ThreadBound::new(service, name)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    }
}

thread_local! {
    /// Objects [`make`] made, waiting for the provider's factory of their registration.
    static MADE: RefCell<HashMap<u64, JsHandle>> = RefCell::new(HashMap::new());
    /// Set by a TS service's factory refusing to run outside a call from JS.
    static NOT_MADE: Cell<bool> = const { Cell::new(false) };
}

/// Registers `service` in `provider` with `lifetime`.
pub(crate) fn add(
    provider: &mut rayx_app::ServiceProvider,
    lifetime: ServiceLifetime,
    service: SharedTsService,
) -> Result<(), String> {
    let key = service
        .get()
        .map_err(|error| error.to_string())?
        .key
        .clone();
    let factory = resolver(service, lifetime);
    match lifetime {
        ServiceLifetime::Singleton => provider.add_singleton_thread_bound_keyed(key, factory),
        ServiceLifetime::Scoped => provider.add_scoped_thread_bound_keyed(key, factory),
        ServiceLifetime::Transient => provider.add_transient_thread_bound_keyed(key, factory),
    };
    Ok(())
}

/// The provider's factory of `service`: the object [`make`] made for it, else its JS factory
/// called on the spot inside a call from JS.
fn resolver(
    service: SharedTsService,
    lifetime: ServiceLifetime,
) -> impl Fn(&rayx_app::ServiceProvider) -> Result<LocalService, ServiceResolutionError>
+ Send
+ Sync
+ 'static {
    move |provider| {
        // Off the engine thread: the wrong-thread error, before anything runs.
        let service = service.get()?;
        let name = || service.key.name().to_owned();
        let keep = |object: JsHandle| -> LocalService {
            if lifetime == ServiceLifetime::Singleton {
                *service.singleton.borrow_mut() = Some(object.clone());
            }
            Rc::new(object)
        };
        if let Some(made) = MADE.with_borrow_mut(|made| made.remove(&service.id)) {
            return Ok(keep(made));
        }
        if !service.factory.can_call_now() {
            NOT_MADE.set(true);
            return Err(ServiceResolutionError::failed(
                name(),
                "it is a TS service that is not made yet: TS services are made in a call from JS \
                 (a TS `getService`), and startup and contract services by `start()`",
            ));
        }
        let services = provider_for(&service.sys, provider);
        let object = service.factory.call(services).map_err(|error| {
            ServiceResolutionError::failed(name(), format!("its factory threw: {error}"))
        })?;
        Ok(keep(object))
    }
}

/// `service`'s object in `provider` (the root, in `start()`'s startup run), made now if it is
/// not: its factory is called in JS's next turn (with the root `ServiceProvider`) and the object
/// is kept as the provider keeps it (a singleton's once, a scoped one's in the root's scope; a
/// transient one's is this one).
pub(crate) async fn make(
    provider: &rayx_app::ServiceProvider,
    service: &TsService,
) -> Result<JsHandle, String> {
    NOT_MADE.set(false);
    match provider.required_thread_bound_service_by_key(&service.key) {
        Ok(made) => return object(made, &service.key),
        Err(error) if !NOT_MADE.replace(false) => return Err(error.to_string()),
        Err(_) => {}
    }
    let services = root_of(&service.sys, provider.clone());
    let object_made = service
        .factory
        .call_async(services)
        .await
        .map_err(|error| format!("its factory threw: {error}"))?;
    MADE.with_borrow_mut(|made| made.insert(service.id, object_made));
    let resolved = provider.required_thread_bound_service_by_key(&service.key);
    // Not taken when the provider had one already (made meanwhile).
    MADE.with_borrow_mut(|made| made.remove(&service.id));
    object(resolved.map_err(|error| error.to_string())?, &service.key)
}

/// The JS object a TS service's value is.
pub(crate) fn object(value: LocalService, key: &RuntimeKey) -> Result<JsHandle, String> {
    value
        .downcast::<JsHandle>()
        .map(|object| (*object).clone())
        .map_err(|_| format!("the service {key} is no JS object"))
}

/// A startup service that is initialized already: what a startup factory gives for the work
/// the run does while it makes its services (validating TS options on start, making a TS
/// contract service), which nothing else initializes.
pub(crate) struct Made;

impl rayx_app::di::AsyncInitializable for Made {
    fn is_initialized(&self) -> bool {
        true
    }

    fn initialize(
        &self,
        _context: rayx_app::di::StartupInitializationContext,
    ) -> futures::future::LocalBoxFuture<'static, rayx_app::di::StartupInitializationResult> {
        Box::pin(futures::future::ready(Ok(())))
    }
}

impl rayx_app::di::StartupInitializable for Made {}

/// Makes `service` (a TS singleton registered under a contract's token) in the startup run,
/// then `made` (the contract's adapter), keyed `key` (the contract's startup key, so Rust
/// startup services depending on the contract come after it).
pub(crate) fn make_on_start(
    provider: &mut rayx_app::ServiceProvider,
    service: SharedTsService,
    key: rayx_app::di::StartupServiceType,
    made: fn(&rayx_app::ServiceProvider),
) {
    use futures::FutureExt as _;
    provider.add_startup_resolved_async(key.clone(), move |context| {
        let service = Arc::clone(&service);
        let root = context.services().clone();
        let key = key.clone();
        async move {
            let failed = |message: String| rayx_app::di::StartupInitializationFailure {
                service: key.clone(),
                message,
                timed_out: false,
            };
            let registration = service.get().map_err(|error| failed(error.to_string()))?;
            make(&root, &registration).await.map_err(failed)?;
            made(&root);
            Ok(Arc::new(Made) as rayx_app::di::StartupService)
        }
        .boxed_local() as rayx_app::di::StartupServiceFuture
    });
}
