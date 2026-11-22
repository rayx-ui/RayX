//! The registrations of a scope of its own ([`ServiceProvider::create_scope_with`]).

use super::key::{ErasedService, RuntimeKey, ServiceKey};
use super::provider::{IntoServiceArc, IntoSingletonService, ServiceProvider};
use super::ready::DiReady;
use super::registry::{
    ServiceDescriptor, ServiceLifetime, ServiceResolutionError, erase_service, thread_bound_factory,
};
use super::thread_bound::LocalService;
use std::any::type_name;
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// What [`ServiceProvider::create_scope_with`]'s `configure` registers in the new scope:
/// singletons, scoped and transient services, by Rust type (`add_*`, `add_*_as` for a
/// `dyn Trait`) or by [`RuntimeKey`] (`add_*_keyed`), thread-bound ones included. The last
/// registration of a key wins.
///
/// It has no startup registrations: startup services belong to the root, which initializes
/// them once when the app starts.
///
/// ```compile_fail
/// use rayx_app::prelude::*;
///
/// #[derive(AppService)]
/// struct Warmup;
///
/// ServiceProvider::new().create_scope_with(|scope| {
///     scope.add_startup_singleton(Warmup);
/// });
/// ```
#[derive(Default)]
pub struct ScopeRegistrations {
    descriptors: HashMap<ServiceKey, ServiceDescriptor>,
}

impl ScopeRegistrations {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn into_descriptors(self) -> HashMap<ServiceKey, ServiceDescriptor> {
        self.descriptors
    }

    /// Whether the scope registers anything under the Rust type `T`.
    pub fn contains<T>(&self) -> bool
    where
        T: ?Sized + 'static,
    {
        self.descriptors.contains_key(&ServiceKey::of::<T>())
    }

    /// Whether the scope registers anything under the runtime key `key`.
    pub fn contains_key(&self, key: &RuntimeKey) -> bool {
        self.descriptors
            .contains_key(&ServiceKey::Runtime(key.clone()))
    }

    /// Registers `service` as the scope's singleton `T`.
    pub fn add_singleton<T>(&mut self, service: impl IntoSingletonService<T>) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        self.add_singleton_as::<T>(service.into_service_arc())
    }

    /// Registers `service` as the scope's singleton `T` (a `dyn Trait` it implements).
    pub fn add_singleton_as<T>(&mut self, service: impl IntoServiceArc<T>) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        let service = service.into_service_arc();
        self.insert(
            ServiceKey::of::<T>(),
            ServiceLifetime::Singleton,
            Arc::new(move |_| Ok(erase_service(Arc::clone(&service)))),
        )
    }

    /// Registers the scope's singleton `T`, made by `factory` with the scope on first resolve.
    pub fn add_singleton_factory<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady,
    {
        self.add_singleton_factory_as::<T>(move |services| Arc::new(factory(services)))
    }

    /// [`add_singleton_factory`](Self::add_singleton_factory) for a `T` (a `dyn Trait`) the
    /// factory gives as an `Arc`.
    pub fn add_singleton_factory_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        self.insert_typed::<T>(ServiceLifetime::Singleton, factory)
    }

    /// Registers the scoped service `T`: one per scope (this one and each of its descendants),
    /// made by `factory` with the scope it is resolved in.
    pub fn add_scoped<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady,
    {
        self.add_scoped_as::<T>(move |services| Arc::new(factory(services)))
    }

    /// [`add_scoped`](Self::add_scoped) for a `T` (a `dyn Trait`) the factory gives as an `Arc`.
    pub fn add_scoped_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        self.insert_typed::<T>(ServiceLifetime::Scoped, factory)
    }

    /// Registers the transient service `T`: `factory` runs on every resolve, with the provider
    /// it is resolved from.
    pub fn add_transient<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady,
    {
        self.add_transient_as::<T>(move |services| Arc::new(factory(services)))
    }

    /// [`add_transient`](Self::add_transient) for a `T` (a `dyn Trait`) the factory gives as an
    /// `Arc`.
    pub fn add_transient_as<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: DiReady + ?Sized,
    {
        self.insert_typed::<T>(ServiceLifetime::Transient, factory)
    }

    /// Registers the scope's singleton under the runtime key `key`, made by `factory` (a
    /// failure is not cached).
    pub fn add_singleton_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.insert(
            ServiceKey::Runtime(key),
            ServiceLifetime::Singleton,
            Arc::new(factory),
        )
    }

    /// Registers a scoped service under the runtime key `key`.
    pub fn add_scoped_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.insert(
            ServiceKey::Runtime(key),
            ServiceLifetime::Scoped,
            Arc::new(factory),
        )
    }

    /// Registers a transient service under the runtime key `key`.
    pub fn add_transient_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<ErasedService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.insert(
            ServiceKey::Runtime(key),
            ServiceLifetime::Transient,
            Arc::new(factory),
        )
    }

    /// Registers the scope's thread-bound singleton `T` (neither `Send` nor `Sync`), made on
    /// the first resolving thread, as [`ServiceProvider::add_singleton_thread_bound`] does.
    pub fn add_singleton_thread_bound<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.insert_thread_bound_typed::<T>(ServiceLifetime::Singleton, factory)
    }

    /// Registers the thread-bound scoped service `T`.
    pub fn add_scoped_thread_bound<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.insert_thread_bound_typed::<T>(ServiceLifetime::Scoped, factory)
    }

    /// Registers the thread-bound transient service `T`.
    pub fn add_transient_thread_bound<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        self.insert_thread_bound_typed::<T>(ServiceLifetime::Transient, factory)
    }

    /// Registers the scope's thread-bound singleton under the runtime key `key`.
    pub fn add_singleton_thread_bound_keyed(
        &mut self,
        key: RuntimeKey,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        self.insert_thread_bound_keyed(key, ServiceLifetime::Singleton, factory)
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
        self.insert_thread_bound_keyed(key, ServiceLifetime::Scoped, factory)
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
        self.insert_thread_bound_keyed(key, ServiceLifetime::Transient, factory)
    }

    fn insert_typed<T>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert(
            ServiceKey::of::<T>(),
            lifetime,
            Arc::new(move |services| Ok(erase_service(factory(services)))),
        )
    }

    fn insert_thread_bound_typed<T>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> T + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: 'static,
    {
        let factory = thread_bound_factory(Cow::Borrowed(type_name::<T>()), move |services| {
            let value: LocalService = Rc::new(factory(services));
            Ok(value)
        });
        self.descriptors.insert(
            ServiceKey::of::<T>(),
            ServiceDescriptor::new(lifetime, true, factory),
        );
        self
    }

    fn insert_thread_bound_keyed(
        &mut self,
        key: RuntimeKey,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Result<LocalService, ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        let factory = thread_bound_factory(key.name_cow(), factory);
        self.descriptors.insert(
            ServiceKey::Runtime(key),
            ServiceDescriptor::new(lifetime, true, factory),
        );
        self
    }

    fn insert(
        &mut self,
        key: ServiceKey,
        lifetime: ServiceLifetime,
        factory: super::registry::ServiceFactory,
    ) -> &mut Self {
        self.descriptors
            .insert(key, ServiceDescriptor::new(lifetime, false, factory));
        self
    }
}

impl std::fmt::Debug for ScopeRegistrations {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScopeRegistrations")
            .field("registrations", &self.descriptors.len())
            .finish()
    }
}
