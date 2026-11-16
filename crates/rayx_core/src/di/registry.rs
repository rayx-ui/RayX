use super::key::{ErasedService, RuntimeKey, ServiceKey};
use super::provider::ServiceProvider;
use super::thread_bound::{LocalService, ThreadBoundSlot};
use std::any::type_name;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, Weak};

/// A factory as the registry keeps it: typed factories never fail, runtime-keyed ones may.
type ServiceFactory =
    Arc<dyn Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError> + Send + Sync>;
type ServiceCache = Mutex<HashMap<ServiceKey, ErasedService>>;
type ResolutionGuards = Mutex<HashMap<ServiceKey, Arc<Mutex<()>>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceLifetime {
    Singleton,
    Scoped,
    Transient,
}

#[derive(Clone)]
struct ServiceDescriptor {
    lifetime: ServiceLifetime,
    /// Made by a thread-bound registration: its cache entries are [`ThreadBoundSlot`]s.
    thread_bound: bool,
    factory: ServiceFactory,
}

/// Which API a resolve comes through: the shared one (`service`, `service_by_key`) or the
/// thread-bound one. A registration of the other kind is refused before its factory runs.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Access {
    Shared,
    ThreadBound,
}

/// The registrations and instance caches of a provider, keyed by [`ServiceKey`].
///
/// Typed services are stored as an erased `Arc<T>`; runtime-keyed services as the erased value
/// their factory made. Both kinds share the caches, the per-key resolution guards and the scope
/// rules.
pub struct ServiceRegistry {
    singletons: Arc<ServiceCache>,
    singleton_resolution_guards: Arc<ResolutionGuards>,
    scoped: Arc<ServiceCache>,
    scoped_resolution_guards: Arc<ResolutionGuards>,
    descriptors: HashMap<ServiceKey, ServiceDescriptor>,
}

impl Clone for ServiceRegistry {
    fn clone(&self) -> Self {
        Self {
            singletons: Arc::clone(&self.singletons),
            singleton_resolution_guards: Arc::clone(&self.singleton_resolution_guards),
            scoped: Arc::clone(&self.scoped),
            scoped_resolution_guards: Arc::clone(&self.scoped_resolution_guards),
            descriptors: self.descriptors.clone(),
        }
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self {
            singletons: Arc::new(Mutex::new(HashMap::new())),
            singleton_resolution_guards: Arc::new(Mutex::new(HashMap::new())),
            scoped: Arc::new(Mutex::new(HashMap::new())),
            scoped_resolution_guards: Arc::new(Mutex::new(HashMap::new())),
            descriptors: HashMap::new(),
        }
    }
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T>(&mut self, service: Arc<T>) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_erased(ServiceKey::of::<T>(), erase_service(service))
    }

    pub fn contains<T>(&self) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.contains_key(&ServiceKey::of::<T>())
    }

    /// Whether anything is registered (or cached) under `key`.
    pub fn contains_key(&self, key: &ServiceKey) -> bool {
        self.descriptors.contains_key(key)
            || self
                .singletons
                .lock()
                .is_ok_and(|services| services.contains_key(key))
            || self
                .scoped
                .lock()
                .is_ok_and(|services| services.contains_key(key))
    }

    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.cached(&ServiceKey::of::<T>())
            .as_ref()
            .and_then(downcast_service::<T>)
    }

    pub fn required<T>(&self) -> Result<Arc<T>, ServiceResolutionError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get::<T>()
            .ok_or_else(ServiceResolutionError::missing::<T>)
    }

    pub fn insert_singleton_factory<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_typed_descriptor::<T>(ServiceLifetime::Singleton, factory)
    }

    pub fn insert_scoped<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_typed_descriptor::<T>(ServiceLifetime::Scoped, factory)
    }

    pub fn insert_transient<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_typed_descriptor::<T>(ServiceLifetime::Transient, factory)
    }

    /// Registers a runtime-keyed service: `factory` makes its erased value, or fails.
    pub fn insert_keyed(
        &mut self,
        key: RuntimeKey,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.insert_descriptor(ServiceKey::Runtime(key), lifetime, false, Arc::new(factory))
    }

    /// Registers a thread-bound service under `key`: `factory` makes its value on the resolving
    /// thread, and the value stays on that thread ([`super::thread_bound`]).
    pub(crate) fn insert_thread_bound(
        &mut self,
        key: ServiceKey,
        service: Cow<'static, str>,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.insert_descriptor(
            key,
            lifetime,
            true,
            Arc::new(move |provider| {
                let value = factory(provider)?;
                let slot: ErasedService = Arc::new(ThreadBoundSlot::store(value, service.clone())?);
                Ok(slot)
            }),
        )
    }

    /// The thread-bound service under `key`, on the thread that made it: `None` when nothing is
    /// registered under it.
    pub(crate) fn resolve_thread_bound(
        &self,
        key: &ServiceKey,
        service: impl Fn() -> Cow<'static, str>,
        provider: &ServiceProvider,
    ) -> Option<Result<LocalService, ServiceResolutionError>> {
        Some(
            self.resolve_erased(key, Access::ThreadBound, &service, provider)?
                .and_then(|erased| match erased.downcast_ref::<ThreadBoundSlot>() {
                    Some(slot) => slot.get(),
                    None => Err(Access::ThreadBound.refused(service())),
                }),
        )
    }

    pub(crate) fn resolve<T>(&self, provider: &ServiceProvider) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.resolve_typed::<T>(provider)?.ok()
    }

    fn resolve_typed<T>(
        &self,
        provider: &ServiceProvider,
    ) -> Option<Result<Arc<T>, ServiceResolutionError>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let service = || Cow::Borrowed(type_name::<T>());
        Some(
            self.resolve_erased(&ServiceKey::of::<T>(), Access::Shared, service, provider)?
                .and_then(|erased| {
                    downcast_service::<T>(&erased).ok_or_else(|| {
                        ServiceResolutionError::failed(service(), "its value has another type")
                    })
                }),
        )
    }

    pub(crate) fn required_resolved<T>(
        &self,
        provider: &ServiceProvider,
    ) -> Result<Arc<T>, ServiceResolutionError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.resolve_typed::<T>(provider)
            .unwrap_or_else(|| Err(ServiceResolutionError::missing::<T>()))
    }

    pub(crate) fn required_resolved_by_key(
        &self,
        key: &RuntimeKey,
        provider: &ServiceProvider,
    ) -> Result<ErasedService, ServiceResolutionError> {
        let service = || key.name_cow();
        let key = ServiceKey::Runtime(key.clone());
        self.resolve_erased(&key, Access::Shared, service, provider)
            .unwrap_or_else(|| Err(ServiceResolutionError::MissingKey { service: service() }))
    }

    /// Whether `other` shares this registry's scoped cache (a clone of the same scope).
    pub(crate) fn same_scope(&self, other: &ServiceRegistry) -> bool {
        Arc::ptr_eq(&self.scoped, &other.scoped)
    }

    /// This registry without keeping its caches alive ([`WeakServiceRegistry::upgrade`]).
    pub(crate) fn downgrade(&self) -> WeakServiceRegistry {
        WeakServiceRegistry {
            singletons: Arc::downgrade(&self.singletons),
            singleton_resolution_guards: Arc::downgrade(&self.singleton_resolution_guards),
            scoped: Arc::downgrade(&self.scoped),
            scoped_resolution_guards: Arc::downgrade(&self.scoped_resolution_guards),
            descriptors: self.descriptors.clone(),
        }
    }

    pub(crate) fn create_scope(&self) -> Self {
        Self {
            singletons: Arc::clone(&self.singletons),
            singleton_resolution_guards: Arc::clone(&self.singleton_resolution_guards),
            scoped: Arc::new(Mutex::new(HashMap::new())),
            scoped_resolution_guards: Arc::new(Mutex::new(HashMap::new())),
            descriptors: self.descriptors.clone(),
        }
    }

    /// The service under `key`: `None` when nothing is registered under it, else what its
    /// cache or factory gives.
    fn resolve_erased(
        &self,
        key: &ServiceKey,
        access: Access,
        service: impl Fn() -> Cow<'static, str>,
        provider: &ServiceProvider,
    ) -> Option<Result<ErasedService, ServiceResolutionError>> {
        // The kind is checked first: a resolve through the other API makes and reads nothing.
        let descriptor = self.descriptors.get(key).cloned();
        let thread_bound = descriptor
            .as_ref()
            .is_some_and(|descriptor| descriptor.thread_bound);
        if thread_bound != (access == Access::ThreadBound)
            && (descriptor.is_some() || self.cached(key).is_some())
        {
            return Some(Err(access.refused(service())));
        }

        if let Some(service) = self.cached(key) {
            return Some(Ok(service));
        }

        let descriptor = descriptor?;
        Some(match descriptor.lifetime {
            ServiceLifetime::Singleton => self.resolve_cached(
                key,
                &self.singletons,
                &self.singleton_resolution_guards,
                provider,
                descriptor.factory,
            ),
            ServiceLifetime::Scoped => self.resolve_cached(
                key,
                &self.scoped,
                &self.scoped_resolution_guards,
                provider,
                descriptor.factory,
            ),
            ServiceLifetime::Transient => (descriptor.factory)(provider),
        })
    }

    fn insert_erased(&mut self, key: ServiceKey, service: ErasedService) -> &mut Self {
        self.descriptors.remove(&key);
        self.scoped
            .lock()
            .ok()
            .and_then(|mut services| services.remove(&key));
        if let Ok(mut services) = self.singletons.lock() {
            services.insert(key, service);
        }
        self
    }

    fn insert_typed_descriptor<T>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_descriptor(
            ServiceKey::of::<T>(),
            lifetime,
            false,
            Arc::new(move |provider| Ok(erase_service(factory(provider)))),
        )
    }

    fn insert_descriptor(
        &mut self,
        key: ServiceKey,
        lifetime: ServiceLifetime,
        thread_bound: bool,
        factory: ServiceFactory,
    ) -> &mut Self {
        if let Ok(mut services) = self.singletons.lock() {
            services.remove(&key);
        }
        if let Ok(mut services) = self.scoped.lock() {
            services.remove(&key);
        }
        self.descriptors.insert(
            key,
            ServiceDescriptor {
                lifetime,
                thread_bound,
                factory,
            },
        );
        self
    }

    fn cached(&self, key: &ServiceKey) -> Option<ErasedService> {
        cached_from(&self.singletons, key).or_else(|| cached_from(&self.scoped, key))
    }

    fn resolve_cached(
        &self,
        key: &ServiceKey,
        cache: &ServiceCache,
        resolution_guards: &ResolutionGuards,
        provider: &ServiceProvider,
        factory: ServiceFactory,
    ) -> Result<ErasedService, ServiceResolutionError> {
        if let Some(service) = cached_from(cache, key) {
            return Ok(service);
        }

        let poisoned = || ServiceResolutionError::poisoned(key);
        let resolution_guard = resolution_guards
            .lock()
            .map_err(|_| poisoned())?
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _resolution = resolution_guard.lock().map_err(|_| poisoned())?;
        if let Some(service) = cached_from(cache, key) {
            return Ok(service);
        }

        let value = factory(provider)?;
        if let Ok(mut services) = cache.lock() {
            services.insert(key.clone(), Arc::clone(&value));
        }
        Ok(value)
    }
}

/// A [`ServiceRegistry`] that does not keep its caches alive.
#[derive(Clone)]
pub(crate) struct WeakServiceRegistry {
    singletons: Weak<ServiceCache>,
    singleton_resolution_guards: Weak<ResolutionGuards>,
    scoped: Weak<ServiceCache>,
    scoped_resolution_guards: Weak<ResolutionGuards>,
    descriptors: HashMap<ServiceKey, ServiceDescriptor>,
}

impl WeakServiceRegistry {
    /// The registry, while its scope lives.
    pub(crate) fn upgrade(&self) -> Option<ServiceRegistry> {
        Some(ServiceRegistry {
            singletons: self.singletons.upgrade()?,
            singleton_resolution_guards: self.singleton_resolution_guards.upgrade()?,
            scoped: self.scoped.upgrade()?,
            scoped_resolution_guards: self.scoped_resolution_guards.upgrade()?,
            descriptors: self.descriptors.clone(),
        })
    }
}

impl Access {
    /// The error of a resolve through this API of a registration of the other kind.
    fn refused(self, service: Cow<'static, str>) -> ServiceResolutionError {
        let message = match self {
            Self::Shared => {
                "it is thread-bound: resolve it with `thread_bound_service` or `thread_bound_service_by_key`"
            }
            Self::ThreadBound => {
                "it is not thread-bound: resolve it with `service` or `service_by_key`"
            }
        };
        ServiceResolutionError::failed(service, message)
    }
}

fn key_name(key: &ServiceKey) -> Cow<'static, str> {
    match key {
        ServiceKey::Type(_) => Cow::Borrowed("<type>"),
        ServiceKey::Runtime(key) => key.name_cow(),
    }
}

fn cached_from(services: &ServiceCache, key: &ServiceKey) -> Option<ErasedService> {
    services
        .lock()
        .ok()
        .and_then(|services| services.get(key).cloned())
}

impl fmt::Debug for ServiceRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let singleton_len = self
            .singletons
            .lock()
            .map(|services| services.len())
            .unwrap_or_default();
        let scoped_len = self
            .scoped
            .lock()
            .map(|services| services.len())
            .unwrap_or_default();
        formatter
            .debug_struct("ServiceRegistry")
            .field("singletons", &singleton_len)
            .field("scoped", &scoped_len)
            .field("descriptors", &self.descriptors.len())
            .finish()
    }
}

/// Why a service did not resolve.
///
/// It is `#[non_exhaustive]`: a later RayX may add a variant without a breaking change, so a
/// `match` over it outside `rayx_core` has a wildcard arm:
///
/// ```
/// use rayx_core::ServiceResolutionError;
///
/// fn kind(error: &ServiceResolutionError) -> &'static str {
///     match error {
///         ServiceResolutionError::Missing { .. } | ServiceResolutionError::MissingKey { .. } => {
///             "missing"
///         }
///         ServiceResolutionError::WrongThread { .. } => "wrong thread",
///         _ => "failed",
///     }
/// }
/// # assert_eq!(kind(&ServiceResolutionError::failed("X", "no")), "failed");
/// ```
///
/// A `match` that names every variant and no wildcard does not compile:
///
/// ```compile_fail
/// use rayx_core::ServiceResolutionError;
///
/// fn kind(error: &ServiceResolutionError) -> &'static str {
///     match error {
///         ServiceResolutionError::Missing { .. } => "missing",
///         ServiceResolutionError::MissingKey { .. } => "missing key",
///         ServiceResolutionError::WrongThread { .. } => "wrong thread",
///         ServiceResolutionError::Failed { .. } => "failed",
///     }
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ServiceResolutionError {
    /// No service is registered under the Rust type `service`.
    Missing { service: &'static str },
    /// No service is registered under the runtime key named `service`.
    MissingKey { service: Cow<'static, str> },
    /// The thread-bound service `service` was resolved on a thread other than the one that made
    /// it; its value was not touched.
    WrongThread { service: Cow<'static, str> },
    /// Resolving `service` failed: its factory gave an error, or a lock it needs was poisoned.
    Failed {
        service: Cow<'static, str>,
        message: String,
    },
}

impl fmt::Display for ServiceResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { service } => write!(formatter, "service `{service}` is not registered"),
            Self::MissingKey { service } => {
                write!(formatter, "service `{service}` is not registered")
            }
            Self::WrongThread { service } => write!(
                formatter,
                "service `{service}` is thread-bound and resolves on its own thread only"
            ),
            Self::Failed { service, message } => {
                write!(
                    formatter,
                    "service `{service}` could not be resolved: {message}"
                )
            }
        }
    }
}

impl std::error::Error for ServiceResolutionError {}

impl ServiceResolutionError {
    /// A factory's failure to make the service named `service`.
    pub fn failed(service: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Self::Failed {
            service: service.into(),
            message: message.into(),
        }
    }

    pub(crate) fn missing<T>() -> Self
    where
        T: ?Sized + 'static,
    {
        Self::Missing {
            service: type_name::<T>(),
        }
    }

    pub(crate) fn missing_key(key: &RuntimeKey) -> Self {
        Self::MissingKey {
            service: key.name_cow(),
        }
    }

    fn poisoned(key: &ServiceKey) -> Self {
        Self::failed(key_name(key), "a resolution lock was poisoned")
    }
}

fn erase_service<T>(service: Arc<T>) -> ErasedService
where
    T: ?Sized + Send + Sync + 'static,
{
    Arc::new(service)
}

fn downcast_service<T>(service: &ErasedService) -> Option<Arc<T>>
where
    T: ?Sized + Send + Sync + 'static,
{
    service.downcast_ref::<Arc<T>>().cloned()
}
