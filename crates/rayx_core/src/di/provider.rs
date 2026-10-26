use super::lifecycle::{StartupInitializable, StartupRegistry, StartupServiceType};
use super::ready::DiReady;
use super::registry::{ServiceRegistry, ServiceResolutionError};
use crate::options::{Options, OptionsError, OptionsRegistry};
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

/// The service container: registrations, options and the startup registry.
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
#[derive(Clone, Debug)]
pub struct ServiceProvider {
    options: OptionsRegistry,
    services: ServiceRegistry,
    startup: StartupRegistry,
}

impl ServiceProvider {
    pub fn new() -> Self {
        Self {
            options: OptionsRegistry::default(),
            services: ServiceRegistry::default(),
            startup: StartupRegistry::default(),
        }
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn options<T>(&self) -> Option<Options<T>>
    where
        T: Send + Sync + 'static,
    {
        self.options.options::<T>()
    }

    pub fn required_options<T>(&self) -> Result<Options<T>, OptionsError>
    where
        T: Send + Sync + 'static,
    {
        self.options.required::<T>()
    }

    pub fn options_registry(&self) -> &OptionsRegistry {
        &self.options
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

    pub fn with_options_registry(mut self, options: OptionsRegistry) -> Self {
        self.options = options;
        self
    }

    pub fn with_added_options(mut self, options: OptionsRegistry) -> Self {
        self.options.extend(options);
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

    /// Creates a child scope: shared singletons, options and startup registry,
    /// a fresh scoped-instance cache, and a snapshot of the registrations.
    pub fn create_scope(&self) -> Self {
        Self {
            options: self.options.clone(),
            services: self.services.create_scope(),
            startup: self.startup.clone(),
        }
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
