use super::key::{ErasedService, RuntimeKey, ServiceKey};
use super::lifecycle::{
    StartupInitializable, StartupInitializationContext, StartupRegistry, StartupServiceFuture,
    StartupServiceType,
};
use super::ready::DiReady;
use super::registry::{
    ServiceLifetime, ServiceRegistry, ServiceResolutionError, WeakServiceRegistry,
};
use super::thread_bound::LocalService;
use crate::options::OptionsRegistrations;
use std::any::type_name;
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

pub trait IntoServiceArc<T>
where
    T: ?Sized + DiReady,
{
    fn into_service_arc(self) -> Arc<T>;
}

impl<T> IntoServiceArc<T> for T
where
    T: DiReady,
{
    fn into_service_arc(self) -> Arc<T> {
        Arc::new(self)
    }
}

impl<T> IntoServiceArc<T> for Arc<T>
where
    T: ?Sized + DiReady,
{
    fn into_service_arc(self) -> Arc<T> {
        self
    }
}

pub trait IntoSingletonService<T>: IntoServiceArc<T>
where
    T: ?Sized + DiReady,
{
}

impl<TService, T> IntoSingletonService<T> for TService
where
    TService: IntoServiceArc<T>,
    T: ?Sized + DiReady,
{
}

/// The service container: registrations, options registrations and the startup registry.
///
/// Lifetimes (RayX's behaviour, unchanged):
///
/// - **singleton**: one instance per root provider, shared by every scope created
///   from it (directly or transitively) and by every clone.
/// - **scoped**: one instance per scope. [`ServiceProvider::create_scope`] starts a
///   fresh scoped cache; clones of a provider share their scope's cache. The root
///   provider is itself a scope: resolving a scoped service from the root is **not**
///   an error, it returns the root scope's instance (distinct from every child
///   scope's). Dropping the last clone of a scope drops its scoped instances.
/// - **transient**: the factory runs on every resolve.
///
/// Factories receive the provider the service is resolved from, so a singleton
/// factory resolved through a scope sees that scope (as in RayX). Registering on
/// a provider after scopes were created does not affect those scopes'
/// registrations, except that pre-built singletons (`add_singleton`) land in the
/// shared singleton cache.
///
/// Services are keyed by a Rust type or by a [`RuntimeKey`] (`add_*_keyed`,
/// `service_by_key`); both kinds follow the rules above.
#[derive(Clone, Debug)]
pub struct ServiceProvider {
    options: OptionsRegistrations,
    services: ServiceRegistry,
    startup: StartupRegistry,
}

impl ServiceProvider {
    pub fn new() -> Self {
        Self {
            options: OptionsRegistrations::default(),
            services: ServiceRegistry::default(),
            startup: StartupRegistry::default(),
        }
    }

    pub fn empty() -> Self {
        Self::new()
    }

    /// This provider without keeping its scope alive: what something its scope caches keeps
    /// to reach it again (a reference to the provider would keep the scope, and so itself,
    /// alive).
    pub fn downgrade(&self) -> WeakServiceProvider {
        WeakServiceProvider {
            options: self.options.clone(),
            services: self.services.downgrade(),
            startup: self.startup.clone(),
        }
    }

    /// Whether `other` is this provider's scope (a clone of it, sharing its scoped services),
    /// not another scope of the same root.
    pub fn same_scope(&self, other: &ServiceProvider) -> bool {
        self.services.same_scope(&other.services)
    }

    pub(crate) fn options_registrations(&self) -> &OptionsRegistrations {
        &self.options
    }

    pub(crate) fn options_registrations_mut(&mut self) -> &mut OptionsRegistrations {
        &mut self.options
    }

    pub(crate) fn service_registry_mut(&mut self) -> &mut ServiceRegistry {
        &mut self.services
    }

    pub fn service<T>(&self) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.resolve::<T>(self)
    }

    pub fn required_service<T>(&self) -> Result<Arc<T>, ServiceResolutionError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.required_resolved::<T>(self)
    }

    pub fn service_registry(&self) -> &ServiceRegistry {
        &self.services
    }

    /// The runtime-keyed service `key`, or `None` when nothing is registered under it or its
    /// factory failed ([`Self::required_service_by_key`] says why).
    pub fn service_by_key(&self, key: &RuntimeKey) -> Option<ErasedService> {
        self.required_service_by_key(key).ok()
    }

    /// The runtime-keyed service `key`: [`ServiceResolutionError::MissingKey`] (naming the key)
    /// when nothing is registered under it, or its factory's error.
    pub fn required_service_by_key(
        &self,
        key: &RuntimeKey,
    ) -> Result<ErasedService, ServiceResolutionError> {
        self.services.required_resolved_by_key(key, self)
    }

    /// Whether a service is registered under the runtime key `key`.
    pub fn contains_service_by_key(&self, key: &RuntimeKey) -> bool {
        self.services
            .contains_key(&ServiceKey::Runtime(key.clone()))
    }

    /// Registers a singleton under the runtime key `key`: one value per root provider, shared
    /// by every scope, made by `factory` on first resolve (a failure is not cached).
    pub fn add_singleton_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.services
            .insert_keyed(key, ServiceLifetime::Singleton, factory);
        self
    }

    /// Registers a scoped service under the runtime key `key`: one value per scope.
    pub fn add_scoped_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.services
            .insert_keyed(key, ServiceLifetime::Scoped, factory);
        self
    }

    /// Registers a transient service under the runtime key `key`: `factory` runs on every
    /// resolve.
    pub fn add_transient_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.services
            .insert_keyed(key, ServiceLifetime::Transient, factory);
        self
    }

    pub fn contains_service<T>(&self) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.contains::<T>()
    }

    pub fn try_add_service_factory<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> bool
    where
        T: DiReady,
    {
        if self.services.contains::<T>() {
            return false;
        }
        self.startup.remove(StartupServiceType::of::<T>());
        self.services
            .insert_singleton_factory::<T>(move |services| Arc::new(factory(services)));
        true
    }

    pub fn replace_service_factory<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) where
        T: DiReady,
    {
        self.startup.remove(StartupServiceType::of::<T>());
        self.services
            .insert_singleton_factory::<T>(move |services| Arc::new(factory(services)));
    }

    pub fn try_add_service_factory_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> bool
    where
        T: DiReady + ?Sized,
    {
        if self.services.contains::<T>() {
            return false;
        }
        self.startup.remove(StartupServiceType::of::<T>());
        self.services.insert_singleton_factory::<T>(factory);
        true
    }

    pub fn replace_service_factory_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) where
        T: DiReady + ?Sized,
    {
        self.startup.remove(StartupServiceType::of::<T>());
        self.services.insert_singleton_factory::<T>(factory);
    }

    pub fn try_add_service<T>(&mut self, service: impl IntoSingletonService<T>) -> bool
    where
        T: DiReady + ?Sized,
    {
        if self.services.contains::<T>() {
            return false;
        }
        let service = service.into_service_arc();
        self.startup.remove(StartupServiceType::of::<T>());
        self.services.insert(service);
        true
    }

    pub fn replace_service<T>(&mut self, service: impl IntoSingletonService<T>) -> Arc<T>
    where
        T: DiReady + ?Sized,
    {
        let service = service.into_service_arc();
        self.startup.remove(StartupServiceType::of::<T>());
        self.services.insert(Arc::clone(&service));
        service
    }

    pub fn try_add_app_service_factory<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> bool
    where
        T: StartupInitializable,
    {
        if self.services.contains::<T>() {
            return false;
        }
        self.services
            .insert_singleton_factory::<T>(move |services| Arc::new(factory(services)));
        self.startup.add_resolved::<T>();
        true
    }

    pub fn replace_app_service_factory<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) where
        T: StartupInitializable,
    {
        self.services
            .insert_singleton_factory::<T>(move |services| Arc::new(factory(services)));
        self.startup.add_resolved::<T>();
    }

    pub fn try_add_app_service_factory_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> bool
    where
        T: StartupInitializable + ?Sized,
    {
        if self.services.contains::<T>() {
            return false;
        }
        self.services.insert_singleton_factory::<T>(factory);
        self.startup.add_resolved::<T>();
        true
    }

    pub fn replace_app_service_factory_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) where
        T: StartupInitializable + ?Sized,
    {
        self.services.insert_singleton_factory::<T>(factory);
        self.startup.add_resolved::<T>();
    }

    pub fn try_add_app_service<T>(&mut self, service: impl IntoSingletonService<T>) -> bool
    where
        T: StartupInitializable + ?Sized,
    {
        if self.services.contains::<T>() {
            return false;
        }
        let service = service.into_service_arc();
        self.services.insert(service);
        self.startup.add_resolved::<T>();
        true
    }

    pub fn replace_app_service<T>(&mut self, service: impl IntoSingletonService<T>) -> Arc<T>
    where
        T: StartupInitializable + ?Sized,
    {
        let service = service.into_service_arc();
        self.services.insert(Arc::clone(&service));
        self.startup.add_resolved::<T>();
        service
    }

    /// Registers the thread-bound singleton `T`, a value that is neither `Send` nor `Sync`:
    /// `factory` makes it on the first resolving thread, and it resolves on that thread only
    /// ([`Self::thread_bound_service`]); elsewhere resolving fails with
    /// [`ServiceResolutionError::WrongThread`]. It is never dropped off its thread.
    pub fn add_singleton_thread_bound<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.add_thread_bound_typed::<T>(ServiceLifetime::Singleton, factory)
    }

    /// Registers the thread-bound scoped service `T`: one value per scope, made on the thread
    /// that first resolves it there.
    pub fn add_scoped_thread_bound<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.add_thread_bound_typed::<T>(ServiceLifetime::Scoped, factory)
    }

    /// Registers the thread-bound transient service `T`: a new value per resolve, on the
    /// resolving thread.
    pub fn add_transient_thread_bound<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.add_thread_bound_typed::<T>(ServiceLifetime::Transient, factory)
    }

    /// Registers a thread-bound singleton under the runtime key `key`.
    pub fn add_singleton_thread_bound_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.add_thread_bound_keyed(key, ServiceLifetime::Singleton, factory)
    }

    /// Registers a thread-bound scoped service under the runtime key `key`.
    pub fn add_scoped_thread_bound_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.add_thread_bound_keyed(key, ServiceLifetime::Scoped, factory)
    }

    /// Registers a thread-bound transient service under the runtime key `key`.
    pub fn add_transient_thread_bound_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.add_thread_bound_keyed(key, ServiceLifetime::Transient, factory)
    }

    /// The thread-bound service `T`, or `None` when it is not registered, lives on another
    /// thread or failed ([`Self::required_thread_bound_service`] says why).
    pub fn thread_bound_service<T>(&self) -> Option<Rc<T>>
    where
        T: 'static,
    {
        self.required_thread_bound_service::<T>().ok()
    }

    /// The thread-bound service `T`, on the thread that made it.
    pub fn required_thread_bound_service<T>(&self) -> Result<Rc<T>, ServiceResolutionError>
    where
        T: 'static,
    {
        let service = || Cow::Borrowed(type_name::<T>());
        self.services
            .resolve_thread_bound(&ServiceKey::of::<T>(), service, self)
            .unwrap_or_else(|| Err(ServiceResolutionError::missing::<T>()))?
            .downcast::<T>()
            .map_err(|_| ServiceResolutionError::failed(service(), "its value has another type"))
    }

    /// The thread-bound service under the runtime key `key`, or `None` when nothing is
    /// registered under it, it lives on another thread or it failed.
    pub fn thread_bound_service_by_key(&self, key: &RuntimeKey) -> Option<LocalService> {
        self.required_thread_bound_service_by_key(key).ok()
    }

    /// The thread-bound service under the runtime key `key`, on the thread that made it.
    pub fn required_thread_bound_service_by_key(
        &self,
        key: &RuntimeKey,
    ) -> Result<LocalService, ServiceResolutionError> {
        self.services
            .resolve_thread_bound(&ServiceKey::Runtime(key.clone()), || key.name_cow(), self)
            .unwrap_or_else(|| Err(ServiceResolutionError::missing_key(key)))
    }

    fn add_thread_bound_typed<T>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.services.insert_thread_bound(
            ServiceKey::of::<T>(),
            Cow::Borrowed(type_name::<T>()),
            lifetime,
            move |services| {
                let value: LocalService = Rc::new(factory(services));
                Ok(value)
            },
        );
        self
    }

    fn add_thread_bound_keyed(
        &mut self,
        key: RuntimeKey,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        let service = key.name_cow();
        self.services
            .insert_thread_bound(ServiceKey::Runtime(key), service, lifetime, factory);
        self
    }

    pub fn has_startup_role<T>(&self) -> bool
    where
        T: ?Sized + 'static,
    {
        self.startup.contains(StartupServiceType::of::<T>())
    }

    pub fn with_service<T>(mut self, service: Arc<T>) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.insert(service);
        self
    }

    pub fn add_service<T>(&mut self, service: Arc<T>) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.insert(service);
        self
    }

    pub fn with_singleton<T>(mut self, service: impl IntoSingletonService<T>) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.add_singleton::<T>(service);
        self
    }

    pub fn add_singleton<T>(&mut self, service: impl IntoSingletonService<T>) -> Arc<T>
    where
        T: DiReady + ?Sized,
    {
        let service = service.into_service_arc();
        self.services.insert(Arc::clone(&service));
        service
    }

    pub fn with_singleton_as<T>(mut self, service: impl IntoServiceArc<T>) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.add_singleton_as::<T>(service);
        self
    }

    pub fn add_singleton_as<T>(&mut self, service: impl IntoServiceArc<T>) -> Arc<T>
    where
        T: DiReady + ?Sized,
    {
        let service = service.into_service_arc();
        self.services.insert(Arc::clone(&service));
        service
    }

    pub fn with_scoped<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady,
    {
        self.add_scoped::<T>(factory);
        self
    }

    pub fn add_scoped<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady,
    {
        self.services
            .insert_scoped::<T>(move |services| Arc::new(factory(services)));
        self
    }

    pub fn with_scoped_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.add_scoped_as::<T>(factory);
        self
    }

    pub fn add_scoped_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        self.services.insert_scoped::<T>(factory);
        self
    }

    pub fn with_transient<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady,
    {
        self.add_transient::<T>(factory);
        self
    }

    pub fn add_transient<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady,
    {
        self.services
            .insert_transient::<T>(move |services| Arc::new(factory(services)));
        self
    }

    pub fn with_transient_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: DiReady + ?Sized,
    {
        self.add_transient_as::<T>(factory);
        self
    }

    pub fn add_transient_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        self.services.insert_transient::<T>(factory);
        self
    }

    pub fn with_startup_singleton<T>(mut self, service: impl IntoSingletonService<T>) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.add_startup_singleton::<T>(service);
        self
    }

    pub fn add_startup_singleton<T>(&mut self, service: impl IntoSingletonService<T>) -> Arc<T>
    where
        T: StartupInitializable + ?Sized,
    {
        let service = service.into_service_arc();
        self.services.insert(Arc::clone(&service));
        self.startup.add_shared(Arc::clone(&service));
        service
    }

    pub fn with_startup_singleton_as<T>(mut self, service: impl IntoServiceArc<T>) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.add_startup_singleton_as::<T>(service);
        self
    }

    pub fn add_startup_singleton_as<T>(&mut self, service: impl IntoServiceArc<T>) -> Arc<T>
    where
        T: StartupInitializable + ?Sized,
    {
        let service = service.into_service_arc();
        self.services.insert(Arc::clone(&service));
        self.startup.add_shared(Arc::clone(&service));
        service
    }

    pub fn with_startup_scoped<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable,
    {
        self.add_startup_scoped::<T>(factory);
        self
    }

    pub fn add_startup_scoped<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: StartupInitializable,
    {
        self.services
            .insert_scoped::<T>(move |services| Arc::new(factory(services)));
        self.startup.add_resolved::<T>();
        self
    }

    pub fn with_startup_scoped_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.add_startup_scoped_as::<T>(factory);
        self
    }

    pub fn add_startup_scoped_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.services.insert_scoped::<T>(factory);
        self.startup.add_resolved::<T>();
        self
    }

    pub fn with_startup_transient<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable,
    {
        self.add_startup_transient::<T>(factory);
        self
    }

    pub fn add_startup_transient<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: StartupInitializable,
    {
        self.services
            .insert_transient::<T>(move |services| Arc::new(factory(services)));
        self.startup.add_resolved::<T>();
        self
    }

    pub fn with_startup_transient_as<T>(
        mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.add_startup_transient_as::<T>(factory);
        self
    }

    pub fn add_startup_transient_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.services.insert_transient::<T>(factory);
        self.startup.add_resolved::<T>();
        self
    }

    pub fn with_app_service<T>(mut self, service: T) -> Self
    where
        T: StartupInitializable,
    {
        self.add_app_service(service);
        self
    }

    pub fn add_app_service<T>(&mut self, service: T) -> Arc<T>
    where
        T: StartupInitializable,
    {
        self.add_startup_singleton::<T>(service)
    }

    pub fn startup_registry(&self) -> &StartupRegistry {
        &self.startup
    }

    /// The init timeout of startup services that set none
    /// ([`StartupRegistry::set_default_initialization_timeout`]).
    pub fn set_default_initialization_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> &mut Self {
        self.startup.set_default_initialization_timeout(timeout);
        self
    }

    pub fn with_startup_service<T>(mut self, service: Arc<T>) -> Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.startup.add_shared(service);
        self
    }

    pub fn add_startup_service<T>(&mut self, service: Arc<T>) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.startup.add_shared(service);
        self
    }

    /// Adds a startup service made by the asynchronous `factory` when startup runs
    /// ([`StartupRegistry::add_resolved_async`]): keyed by `service_type` (a Rust type or a
    /// [`RuntimeKey`]) unless the service names its own, and run with every other startup
    /// service.
    pub fn add_startup_resolved_async(
        &mut self,
        service_type: impl Into<StartupServiceType>,
        factory: impl Fn(&StartupInitializationContext) -> StartupServiceFuture + Send + Sync + 'static,
    ) -> &mut Self {
        self.startup.add_resolved_async(service_type, factory);
        self
    }

    /// Creates a child scope: shared singletons, options registrations and startup registry,
    /// a fresh scoped-instance cache, and a snapshot of the registrations.
    pub fn create_scope(&self) -> Self {
        Self {
            options: self.options.clone(),
            services: self.services.create_scope(),
            startup: self.startup.clone(),
        }
    }
}

/// A [`ServiceProvider`] that does not keep its scope alive ([`ServiceProvider::downgrade`]).
#[derive(Clone)]
pub struct WeakServiceProvider {
    options: OptionsRegistrations,
    services: WeakServiceRegistry,
    startup: StartupRegistry,
}

impl WeakServiceProvider {
    /// The provider, while its scope lives (some clone of it is kept).
    pub fn upgrade(&self) -> Option<ServiceProvider> {
        Some(ServiceProvider {
            options: self.options.clone(),
            services: self.services.upgrade()?,
            startup: self.startup.clone(),
        })
    }
}

impl std::fmt::Debug for WeakServiceProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WeakServiceProvider")
            .finish_non_exhaustive()
    }
}

impl Default for ServiceProvider {
    fn default() -> Self {
        Self::empty()
    }
}

pub fn default_provider() -> ServiceProvider {
    ServiceProvider::default()
}
