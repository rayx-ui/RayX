use super::key::{ErasedService, RuntimeKey, ServiceKey};
use super::lifecycle::{
    StartupInitializable, StartupInitializationContext, StartupRegistry, StartupServiceFuture,
    StartupServiceType,
};
use super::ready::DiReady;
use super::registry::{
    ServiceLifetime, ServiceRegistry, ServiceResolutionError, WeakServiceRegistry,
};
use super::scope::ScopeRegistrations;
use super::thread_bound::LocalService;
use super::validation::{Requirement, Requirements, ServiceValidationError};
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

/// `Self` (a `dyn Trait`) as the implementation `I` is seen through: what
/// [`ServiceProvider::add_startup_singleton_for`] registers `I` under. A contract
/// (`#[contract]`) implements it for every implementation; for a trait of your own, write
/// `rayx_app::implemented_by!(MyTrait);` beside it.
pub trait ImplementedBy<I> {
    /// `service` as `Self`.
    fn upcast(service: Arc<I>) -> Arc<Self>;
}

/// Implements [`ImplementedBy`] for `dyn Trait` and every implementation of `Trait` that is
/// `'static`: `rayx_app::implemented_by!(MyTrait);`, beside the trait (a contract has it
/// already). Then `add_startup_singleton_for::<dyn MyTrait, _>(..)` registers an implementation.
#[macro_export]
macro_rules! implemented_by {
    ($($trait:ident)::+) => {
        impl<__RayxImplementation> $crate::di::ImplementedBy<__RayxImplementation>
            for dyn $($trait)::+
        where
            __RayxImplementation: $($trait)::+ + 'static,
        {
            fn upcast(
                service: ::std::sync::Arc<__RayxImplementation>,
            ) -> ::std::sync::Arc<Self> {
                service
            }
        }
    };
}

/// What a startup service of the `*_for` forms is resolved as and initialized through: one
/// value, seen as `T` and as a startup service.
struct StartupFor<T>
where
    T: ?Sized,
{
    service: Arc<T>,
    startup: super::lifecycle::StartupService,
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
/// Scoped and transient factories receive the provider the service is resolved from, so they
/// see that scope's scoped services and its own registrations. A root's singleton factory
/// always receives the root (each scope keeps a weak reference to the root it came from), so a
/// singleton first resolved inside a scope never captures that scope's replacements or scoped
/// instances. Registering on a provider after scopes were created does not affect those
/// scopes' registrations, except that pre-built singletons (`add_singleton`) land in the shared
/// singleton cache.
///
/// [`ServiceProvider::create_scope_with`] makes a scope with registrations of its own
/// ([`ScopeRegistrations`]): for their keys they win over the root's, in that scope and its
/// descendants only, and its singletons are one per such scope.
///
/// Services are keyed by a Rust type or by a [`RuntimeKey`] (`add_*_keyed`,
/// `service_by_key`); both kinds follow the rules above.
#[derive(Clone, Debug)]
pub struct ServiceProvider {
    options: OptionsRegistrations,
    services: ServiceRegistry,
    startup: StartupRegistry,
    requirements: Requirements,
    /// The root this scope came from, weakly; `None` for a root.
    root: Option<Arc<WeakServiceProvider>>,
}

impl ServiceProvider {
    pub fn new() -> Self {
        Self {
            options: OptionsRegistrations::default(),
            services: ServiceRegistry::default(),
            startup: StartupRegistry::default(),
            requirements: Requirements::default(),
            root: None,
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
            requirements: self.requirements.clone(),
            root: self.root.clone(),
        }
    }

    /// The root a root singleton resolved here is made with: `None` when this is the root, else
    /// the root while one of its clones lives, else a root over its registrations and the
    /// shared singletons.
    pub(crate) fn root(&self) -> Option<ServiceProvider> {
        let root = self.root.as_deref()?;
        Some(root.upgrade().unwrap_or_else(|| Self {
            options: root.options.clone(),
            services: self.services.detached_root(&root.services),
            startup: root.startup.clone(),
            requirements: root.requirements.clone(),
            root: None,
        }))
    }

    /// The weak reference to the root that this provider's scopes keep.
    fn root_of_scopes(&self) -> Arc<WeakServiceProvider> {
        self.root
            .clone()
            .unwrap_or_else(|| Arc::new(self.downgrade()))
    }

    /// Whether `other` is this provider's scope (a clone of it, sharing its scoped services),
    /// not another scope of the same root.
    pub fn same_scope(&self, other: &ServiceProvider) -> bool {
        self.services.same_scope(&other.services)
    }

    /// Requires `T` (a `dyn Trait`, or any service type): [`validate`](Self::validate), which
    /// `RayXAppBuilder::build` runs first, fails when nothing is registered as `T`; `hint` says
    /// how to register it (`"add_platform() from rayx_services_platform, or register your own
    /// dyn PlatformInfoService"`). What the app or its host needs.
    pub fn require<T>(&mut self, hint: impl Into<Cow<'static, str>>) -> &mut Self
    where
        T: ?Sized + 'static,
    {
        self.requirements.push(Requirement {
            needs: ServiceKey::of::<T>(),
            needs_name: Cow::Borrowed(type_name::<T>()),
            by: None,
            hint: hint.into(),
            attached: false,
        });
        self
    }

    /// Declares that the registration of `S` depends on `T`: [`validate`](Self::validate) fails
    /// when nothing is registered as `T`, naming `S` and `hint`, and when `S` is a singleton and
    /// `T` scoped. What a registration that resolves another service declares, in place of
    /// registering it, **before** registering `S`: the requirement belongs to the next
    /// registration of `S` (or, when none follows, to the current one), and a later registration
    /// of `S` drops it unless that one declares it again, so an app's own `S` registered after a
    /// service crate's needs nothing the crate's needed.
    pub fn require_for<S, T>(&mut self, hint: impl Into<Cow<'static, str>>) -> &mut Self
    where
        S: ?Sized + 'static,
        T: ?Sized + 'static,
    {
        self.requirements.push(Requirement {
            needs: ServiceKey::of::<T>(),
            needs_name: Cow::Borrowed(type_name::<T>()),
            by: Some((ServiceKey::of::<S>(), Cow::Borrowed(type_name::<S>()))),
            hint: hint.into(),
            attached: false,
        });
        self
    }

    /// Checks every requirement ([`require`](Self::require), [`require_for`](Self::require_for))
    /// against what is registered, without making anything: every missing service, and every
    /// singleton depending on a scoped service, in one error.
    pub fn validate(&self) -> Result<(), ServiceValidationError> {
        self.validate_with(&[])
    }

    /// [`validate`](Self::validate), counting the singletons `present` (what is registered once
    /// the checks passed, such as the app's `Configuration`) as registered.
    pub(crate) fn validate_with(
        &self,
        present: &[ServiceKey],
    ) -> Result<(), ServiceValidationError> {
        super::validation::validate(&self.requirements.all(), &|key| {
            if present.contains(key) {
                Some(ServiceLifetime::Singleton)
            } else {
                self.services.lifetime_of(key)
            }
        })
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
        self.registering_key(&key);
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
        self.registering_key(&key);
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
        self.registering_key(&key);
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

    /// Whether anything is registered under `key`.
    pub(crate) fn contains_key(&self, key: &ServiceKey) -> bool {
        self.services.contains_key(key)
    }

    /// What registering a service under `T` does first, so the last registration of a key wins
    /// completely: removes the startup services registered as `T` (a registration that is a
    /// startup service adds its own after), and the requirements the registration of `T` it
    /// replaces declared ([`Self::require_for`]), taking those declared for `T` since as its own.
    fn registering<T>(&mut self)
    where
        T: ?Sized + 'static,
    {
        self.startup
            .remove_registered(&StartupServiceType::of::<T>());
        self.requirements.registered(&ServiceKey::of::<T>());
    }

    /// [`Self::registering`] for the runtime key `key`.
    fn registering_key(&mut self, key: &RuntimeKey) {
        self.startup
            .remove_registered(&StartupServiceType::runtime(key));
        self.requirements
            .registered(&ServiceKey::Runtime(key.clone()));
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
        self.services.insert_singleton_factory::<T>(factory);
        true
    }

    pub fn replace_service_factory_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) where
        T: DiReady + ?Sized,
    {
        self.registering::<T>();
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
        self.registering::<T>();
        self.services.insert(service);
        true
    }

    pub fn replace_service<T>(&mut self, service: impl IntoSingletonService<T>) -> Arc<T>
    where
        T: DiReady + ?Sized,
    {
        let service = service.into_service_arc();
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering_key(&key);
        let service = key.name_cow();
        self.services
            .insert_thread_bound(ServiceKey::Runtime(key), service, lifetime, factory);
        self
    }

    /// Whether a startup service is registered as `T` (`add_startup_singleton_for::<T, _>`, say)
    /// or keyed by `T` (the key it names).
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
        self.registering::<T>();
        self.services.insert(service);
        self
    }

    pub fn add_service<T>(&mut self, service: Arc<T>) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
        let service = service.into_service_arc();
        self.services.insert(Arc::clone(&service));
        self.startup.add_shared(Arc::clone(&service));
        service
    }

    /// [`Self::add_startup_singleton_for`], for chaining.
    pub fn with_startup_singleton_for<T, I>(mut self, service: Arc<I>) -> Self
    where
        T: ?Sized + DiReady + ImplementedBy<I>,
        I: StartupInitializable,
    {
        self.add_startup_singleton_for::<T, I>(service);
        self
    }

    /// Registers `service`, an implementation `I` of `T` (a `dyn Trait`: a contract, or a trait
    /// with [`implemented_by!`](crate::implemented_by)), as the singleton `T` and as a startup
    /// service registered as `T`: resolved as `T`, initialized when startup runs, keyed by its
    /// own [`StartupInitializable::startup_service_type`], else by `T`, so startup services
    /// depending on `T` wait for it. What `add_startup_singleton_as::<dyn Trait>` cannot do (a
    /// trait without `StartupInitializable` as a supertrait). Like every registration of `T`,
    /// it replaces the startup services registered as `T` before, and a later registration of
    /// `T` removes it.
    pub fn add_startup_singleton_for<T, I>(&mut self, service: Arc<I>) -> Arc<I>
    where
        T: ?Sized + DiReady + ImplementedBy<I>,
        I: StartupInitializable,
    {
        self.registering::<T>();
        self.services.insert(T::upcast(Arc::clone(&service)));
        self.startup.add_shared_as::<T, I>(Arc::clone(&service));
        service
    }

    /// [`Self::add_startup_singleton_for`] for the implementation `factory` makes on first
    /// resolve (or when startup runs, whichever is first).
    pub fn add_startup_singleton_factory_for<T, I>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<I> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + DiReady + ImplementedBy<I>,
        I: StartupInitializable,
    {
        self.add_startup_for::<T, I>(ServiceLifetime::Singleton, factory)
    }

    /// [`Self::add_startup_singleton_factory_for`] for a scoped service: one implementation per
    /// scope; startup initializes the root scope's.
    pub fn add_startup_scoped_for<T, I>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<I> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + DiReady + ImplementedBy<I>,
        I: StartupInitializable,
    {
        self.add_startup_for::<T, I>(ServiceLifetime::Scoped, factory)
    }

    /// [`Self::add_startup_singleton_factory_for`] for a transient service: a new implementation
    /// per resolve; startup initializes one it makes.
    pub fn add_startup_transient_for<T, I>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<I> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + DiReady + ImplementedBy<I>,
        I: StartupInitializable,
    {
        self.add_startup_for::<T, I>(ServiceLifetime::Transient, factory)
    }

    /// The `*_for` factory forms: `T` resolves through one hidden registration that makes the
    /// implementation once per `lifetime`, seen as `T` and as a startup service.
    fn add_startup_for<T, I>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Arc<I> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + DiReady + ImplementedBy<I>,
        I: StartupInitializable,
    {
        self.registering::<T>();
        let both = move |services: &ServiceProvider| {
            let service = factory(services);
            Arc::new(StartupFor::<T> {
                service: T::upcast(Arc::clone(&service)),
                startup: service,
            })
        };
        let as_t = |services: &ServiceProvider| {
            let both = services
                .required_service::<StartupFor<T>>()
                .unwrap_or_else(|error| panic!("{error}"));
            Arc::clone(&both.service)
        };
        match lifetime {
            ServiceLifetime::Singleton => {
                self.services
                    .insert_singleton_factory::<StartupFor<T>>(both);
                self.services.insert_singleton_factory::<T>(as_t);
            }
            ServiceLifetime::Scoped => {
                self.services.insert_scoped::<StartupFor<T>>(both);
                self.services.insert_scoped::<T>(as_t);
            }
            ServiceLifetime::Transient => {
                self.services.insert_transient::<StartupFor<T>>(both);
                self.services.insert_transient::<T>(as_t);
            }
        }
        self.startup
            .add_resolved_with(StartupServiceType::of::<T>(), |services| {
                services
                    .required_service::<StartupFor<T>>()
                    .map(|both| Arc::clone(&both.startup))
            });
        self
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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
        self.registering::<T>();
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

    /// Adds `service` to the startup run only (no registration to resolve), registered as `T`
    /// (a `dyn Trait` it stands for, say): keyed by its own
    /// [`StartupInitializable::startup_service_type`], else by `T`, and removed, like every
    /// startup service registered as `T`, when `T` is registered again. Any startup service
    /// registered as `T` before is removed.
    pub fn add_startup_service_for<T, I>(&mut self, service: Arc<I>) -> &mut Self
    where
        T: ?Sized + 'static,
        I: StartupInitializable + ?Sized,
    {
        self.startup.add_shared_as::<T, I>(service);
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

    /// [`Self::add_startup_resolved_async`] for a service the other startup factories may read,
    /// made before any of them runs ([`StartupRegistry::add_resolved_async_first`]): what a TS
    /// implementation of a contract is registered with.
    pub fn add_startup_resolved_async_first(
        &mut self,
        service_type: impl Into<StartupServiceType>,
        factory: impl Fn(&StartupInitializationContext) -> StartupServiceFuture + Send + Sync + 'static,
    ) -> &mut Self {
        self.startup.add_resolved_async_first(service_type, factory);
        self
    }

    /// Creates a child scope: shared singletons, options registrations and startup registry,
    /// a fresh scoped-instance cache, and a snapshot of the registrations.
    pub fn create_scope(&self) -> Self {
        Self {
            options: self.options.clone(),
            services: self.services.create_scope(),
            startup: self.startup.clone(),
            requirements: self.requirements.clone(),
            root: Some(self.root_of_scopes()),
        }
    }

    /// Creates a child scope, as [`create_scope`](Self::create_scope) does, with the
    /// registrations `configure` adds ([`ScopeRegistrations`]: singleton, scoped and transient,
    /// by type or runtime key; startup services belong to the root):
    ///
    /// - For a key it registers, its registration wins in it and its descendants over the
    ///   root's (and over a nearer ancestor's), also when the root made that key already; this
    ///   provider, its other scopes and the root resolve as before.
    /// - A singleton it registers is one instance per scope made here, shared by every
    ///   descendant, and always made with this scope: once this scope is gone, a descendant
    ///   resolving it first gets one made over this scope's registrations and a scoped cache of
    ///   its own, never over the descendant's scoped services. It is dropped when this scope
    ///   and all its descendants are gone.
    /// - Scoped and transient registrations, its own and the root's, resolve their
    ///   dependencies in the provider they are resolved from, so the root's scoped services
    ///   resolved here see this scope's replacements; a root singleton is made with the root.
    ///
    /// A registration whose dependency is not registered fails the resolve with the
    /// [`ServiceResolutionError`] a root registration gets.
    ///
    /// ```
    /// use rayx_app::prelude::*;
    ///
    /// trait Clock: Send + Sync {
    ///     fn now(&self) -> u64;
    /// }
    /// struct SystemClock;
    /// impl Clock for SystemClock {
    ///     fn now(&self) -> u64 { 1_700_000_000 }
    /// }
    /// struct FixedClock(u64);
    /// impl Clock for FixedClock {
    ///     fn now(&self) -> u64 { self.0 }
    /// }
    ///
    /// let mut root = ServiceProvider::new();
    /// root.add_singleton_as::<dyn Clock>(std::sync::Arc::new(SystemClock) as std::sync::Arc<dyn Clock>);
    /// let scope = root.create_scope_with(|scope| {
    ///     scope.add_singleton_as::<dyn Clock>(std::sync::Arc::new(FixedClock(7)) as std::sync::Arc<dyn Clock>);
    /// });
    /// assert_eq!(scope.required_service::<dyn Clock>().unwrap().now(), 7);
    /// assert_eq!(root.required_service::<dyn Clock>().unwrap().now(), 1_700_000_000);
    /// ```
    pub fn create_scope_with(&self, configure: impl FnOnce(&mut ScopeRegistrations)) -> Self {
        let mut registrations = ScopeRegistrations::new();
        configure(&mut registrations);
        let (services, overlay) = self
            .services
            .create_scope_with(registrations.into_descriptors());
        let scope = Self {
            options: self.options.clone(),
            services,
            startup: self.startup.clone(),
            requirements: self.requirements.clone(),
            root: Some(self.root_of_scopes()),
        };
        overlay.declare(scope.downgrade());
        scope
    }
}

/// A [`ServiceProvider`] that does not keep its scope alive ([`ServiceProvider::downgrade`]).
#[derive(Clone)]
pub struct WeakServiceProvider {
    options: OptionsRegistrations,
    services: WeakServiceRegistry,
    startup: StartupRegistry,
    requirements: Requirements,
    root: Option<Arc<WeakServiceProvider>>,
}

impl WeakServiceProvider {
    /// The provider, while its scope lives (some clone of it is kept).
    pub fn upgrade(&self) -> Option<ServiceProvider> {
        Some(ServiceProvider {
            options: self.options.clone(),
            services: self.services.upgrade()?,
            startup: self.startup.clone(),
            requirements: self.requirements.clone(),
            root: self.root.clone(),
        })
    }

    /// Its registrations, for a detached provider over them.
    pub(crate) fn registry(&self) -> &WeakServiceRegistry {
        &self.services
    }

    /// A provider over `services` with this one's options, startup registry, requirements and
    /// root: what a scope's singleton is made with once that scope is gone.
    pub(crate) fn detached(&self, services: ServiceRegistry) -> ServiceProvider {
        ServiceProvider {
            options: self.options.clone(),
            services,
            startup: self.startup.clone(),
            requirements: self.requirements.clone(),
            root: self.root.clone(),
        }
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
