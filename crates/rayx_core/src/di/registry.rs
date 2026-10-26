use super::provider::ServiceProvider;
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

type ErasedService = Arc<dyn Any + Send + Sync>;
type ServiceFactory = Arc<dyn Fn(&ServiceProvider) -> ErasedService + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceLifetime {
    Singleton,
    Scoped,
    Transient,
}

#[derive(Clone)]
struct ServiceDescriptor {
    lifetime: ServiceLifetime,
    factory: ServiceFactory,
}

pub struct ServiceRegistry {
    singletons: Arc<Mutex<HashMap<TypeId, ErasedService>>>,
    singleton_resolution_guards: Arc<Mutex<HashMap<TypeId, Arc<Mutex<()>>>>>,
    scoped: Arc<Mutex<HashMap<TypeId, ErasedService>>>,
    scoped_resolution_guards: Arc<Mutex<HashMap<TypeId, Arc<Mutex<()>>>>>,
    descriptors: HashMap<TypeId, ServiceDescriptor>,
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
        let type_id = TypeId::of::<T>();
        self.descriptors.remove(&type_id);
        self.scoped
            .lock()
            .ok()
            .and_then(|mut services| services.remove(&type_id));
        if let Ok(mut services) = self.singletons.lock() {
            services.insert(type_id, erase_service(service));
        }
        self
    }

    pub fn contains<T>(&self) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        self.descriptors.contains_key(&type_id)
            || self
                .singletons
                .lock()
                .is_ok_and(|services| services.contains_key(&type_id))
            || self
                .scoped
                .lock()
                .is_ok_and(|services| services.contains_key(&type_id))
    }

    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.cached::<T>(TypeId::of::<T>())
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
        self.insert_descriptor::<T>(ServiceLifetime::Singleton, factory)
    }

    pub fn insert_scoped<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_descriptor::<T>(ServiceLifetime::Scoped, factory)
    }

    pub fn insert_transient<T>(
        &mut self,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_descriptor::<T>(ServiceLifetime::Transient, factory)
    }

    pub(crate) fn resolve<T>(&self, provider: &ServiceProvider) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        if let Some(service) = self.cached::<T>(type_id) {
            return Some(service);
        }

        let descriptor = self.descriptors.get(&type_id)?.clone();
        match descriptor.lifetime {
            ServiceLifetime::Singleton => self.resolve_cached::<T>(
                type_id,
                &self.singletons,
                &self.singleton_resolution_guards,
                provider,
                descriptor.factory,
            ),
            ServiceLifetime::Scoped => self.resolve_cached::<T>(
                type_id,
                &self.scoped,
                &self.scoped_resolution_guards,
                provider,
                descriptor.factory,
            ),
            ServiceLifetime::Transient => {
                let value = (descriptor.factory)(provider);
                downcast_service::<T>(&value)
            }
        }
    }

    pub(crate) fn required_resolved<T>(
        &self,
        provider: &ServiceProvider,
    ) -> Result<Arc<T>, ServiceResolutionError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.resolve::<T>(provider)
            .ok_or_else(ServiceResolutionError::missing::<T>)
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

    fn insert_descriptor<T>(
        &mut self,
        lifetime: ServiceLifetime,
        factory: impl Fn(&ServiceProvider) -> Arc<T> + Send + Sync + 'static,
    ) -> &mut Self
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        if let Ok(mut services) = self.singletons.lock() {
            services.remove(&type_id);
        }
        if let Ok(mut services) = self.scoped.lock() {
            services.remove(&type_id);
        }

        self.descriptors.insert(
            type_id,
            ServiceDescriptor {
                lifetime,
                factory: Arc::new(move |provider| erase_service(factory(provider))),
            },
        );
        self
    }

    fn cached<T>(&self, type_id: TypeId) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.cached_from::<T>(&self.singletons, type_id)
            .or_else(|| self.cached_from::<T>(&self.scoped, type_id))
    }

    fn cached_from<T>(
        &self,
        services: &Mutex<HashMap<TypeId, ErasedService>>,
        type_id: TypeId,
    ) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        services
            .lock()
            .ok()
            .and_then(|services| services.get(&type_id).and_then(downcast_service::<T>))
    }

    fn resolve_cached<T>(
        &self,
        type_id: TypeId,
        cache: &Mutex<HashMap<TypeId, ErasedService>>,
        resolution_guards: &Mutex<HashMap<TypeId, Arc<Mutex<()>>>>,
        provider: &ServiceProvider,
        factory: ServiceFactory,
    ) -> Option<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        if let Some(service) = self.cached_from::<T>(cache, type_id) {
            return Some(service);
        }

        let resolution_guard = resolution_guards
            .lock()
            .ok()?
            .entry(type_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _resolution = resolution_guard.lock().ok()?;
        if let Some(service) = self.cached_from::<T>(cache, type_id) {
            return Some(service);
        }

        let value = factory(provider);
        if let Ok(mut services) = cache.lock() {
            services.insert(type_id, Arc::clone(&value));
        }
        downcast_service::<T>(&value)
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceResolutionError {
    Missing { service: &'static str },
}

impl fmt::Display for ServiceResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { service } => write!(formatter, "service `{service}` is not registered"),
        }
    }
}

impl std::error::Error for ServiceResolutionError {}

impl ServiceResolutionError {
    fn missing<T>() -> Self
    where
        T: ?Sized + 'static,
    {
        Self::Missing {
            service: type_name::<T>(),
        }
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
