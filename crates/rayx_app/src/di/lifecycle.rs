use super::key::{RuntimeKey, ServiceKey};
use super::provider::ServiceProvider;
use super::ready::DiReady;
use crate::options::OptionsError;
use futures::FutureExt;
use futures::future::{Either, LocalBoxFuture, join_all, select};
use std::{
    any::type_name,
    borrow::Cow,
    collections::HashSet,
    fmt,
    future::Future,
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// What a startup service is keyed by: a Rust type ([`Self::of`]) or a runtime key
/// ([`Self::runtime`]), with the name failure reports use. Two values are equal when their keys
/// are; a runtime key never equals a type.
#[derive(Clone)]
pub struct StartupServiceType {
    key: ServiceKey,
    name: Cow<'static, str>,
}

impl StartupServiceType {
    pub fn of<T>() -> Self
    where
        T: ?Sized + 'static,
    {
        Self {
            key: ServiceKey::of::<T>(),
            name: Cow::Borrowed(type_name::<T>()),
        }
    }

    /// The startup service keyed by `key`, named by its display name.
    pub fn runtime(key: &RuntimeKey) -> Self {
        Self {
            key: ServiceKey::Runtime(key.clone()),
            name: key.name_cow(),
        }
    }

    pub fn key(&self) -> &ServiceKey {
        &self.key
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl From<&RuntimeKey> for StartupServiceType {
    fn from(key: &RuntimeKey) -> Self {
        Self::runtime(key)
    }
}

impl fmt::Debug for StartupServiceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupServiceType")
            .field("name", &self.name)
            .field("key", &self.key)
            .finish()
    }
}

impl fmt::Display for StartupServiceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

impl PartialEq for StartupServiceType {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for StartupServiceType {}

impl Hash for StartupServiceType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

#[derive(Clone, Debug, Default)]
pub struct StartupInitializationState {
    initialized: Arc<AtomicBool>,
}

impl StartupInitializationState {
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    pub fn mark_initialized(&self) {
        self.initialized.store(true, Ordering::Release);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupServiceError {
    message: String,
}

impl StartupServiceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for StartupServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StartupServiceError {}

impl From<String> for StartupServiceError {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for StartupServiceError {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

pub type StartupInitializationResult = Result<(), StartupServiceError>;

pub trait AsyncInitializable: DiReady {
    fn is_initialized(&self) -> bool;

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult>;

    /// How long [`initialize`](Self::initialize) may take before startup fails with a timeout;
    /// `None` takes the registry's default ([`StartupRegistry::default_initialization_timeout`]).
    /// Enforced only by [`StartupRegistry::initialize_with_timer`].
    fn initialization_timeout(&self) -> Option<Duration> {
        None
    }
}

pub trait StartupInitializable: AsyncInitializable {
    /// The key other startup services name in
    /// [`initialization_depends_on`](Self::initialization_depends_on) to wait for this one.
    /// `None` (the default) keys it by what it was registered as: `add_startup_singleton::<T>`
    /// keys it `T`, `add_startup_singleton_as::<dyn X>` keys it `dyn X` (the scoped and
    /// transient forms alike), [`StartupRegistry::add`] by its concrete type.
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        None
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        Vec::new()
    }
}

impl<T> AsyncInitializable for Arc<T>
where
    T: AsyncInitializable + ?Sized,
{
    fn is_initialized(&self) -> bool {
        self.as_ref().is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.as_ref().initialize(context)
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        self.as_ref().initialization_timeout()
    }
}

impl<T> StartupInitializable for Arc<T>
where
    T: StartupInitializable + ?Sized,
{
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        self.as_ref().startup_service_type()
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        self.as_ref().initialization_depends_on()
    }
}

/// What a startup service receives in [`AsyncInitializable::initialize`].
///
/// RayX's context also carries a `PlatformSnapshot`; the core has no platform
/// layer, so it carries the provider only.
#[derive(Clone, Debug)]
pub struct StartupInitializationContext {
    services: ServiceProvider,
}

impl StartupInitializationContext {
    pub fn new(services: ServiceProvider) -> Self {
        Self { services }
    }

    pub fn services(&self) -> &ServiceProvider {
        &self.services
    }
}

/// A startup service as the registry runs it.
pub type StartupService = Arc<dyn StartupInitializable>;

/// What a startup factory gives: the service, or the failure startup reports.
pub type StartupServiceFuture =
    LocalBoxFuture<'static, Result<StartupService, StartupInitializationFailure>>;

type StartupServiceFactory =
    Arc<dyn Fn(&StartupInitializationContext) -> StartupServiceFuture + Send + Sync>;

#[derive(Clone)]
struct StartupRegistryFactory {
    service_type: StartupServiceType,
    factory: StartupServiceFactory,
    /// Made before the other factories run ([`StartupRegistry::add_resolved_async_first`]).
    first: bool,
}

/// A registered startup service with the key the run orders it by: its own
/// [`StartupInitializable::startup_service_type`], else its registration key.
struct KeyedStartupService {
    key: StartupServiceType,
    /// What it was registered as (its key unless it names its own).
    registered_as: StartupServiceType,
    service: StartupService,
}

impl KeyedStartupService {
    fn new(service: StartupService, registered_as: StartupServiceType) -> Arc<Self> {
        let key = service
            .startup_service_type()
            .unwrap_or_else(|| registered_as.clone());
        Arc::new(Self {
            key,
            registered_as,
            service,
        })
    }
}

impl AsyncInitializable for KeyedStartupService {
    fn is_initialized(&self) -> bool {
        self.service.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.service.initialize(context)
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        self.service.initialization_timeout()
    }
}

impl StartupInitializable for KeyedStartupService {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(self.key.clone())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        self.service.initialization_depends_on()
    }
}

struct SharedStartupService<T>
where
    T: StartupInitializable + ?Sized,
{
    service: Arc<T>,
}

impl<T> AsyncInitializable for SharedStartupService<T>
where
    T: StartupInitializable + ?Sized,
{
    fn is_initialized(&self) -> bool {
        self.service.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.service.initialize(context)
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        self.service.initialization_timeout()
    }
}

impl<T> StartupInitializable for SharedStartupService<T>
where
    T: StartupInitializable + ?Sized,
{
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        self.service.startup_service_type()
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        self.service.initialization_depends_on()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupInitializationFailure {
    pub service: StartupServiceType,
    pub message: String,
    /// The service did not initialize within its timeout.
    pub timed_out: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupInitializationPhase {
    Runtime,
}

impl fmt::Display for StartupInitializationPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime => formatter.write_str("runtime"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartupInitializationError {
    MissingDependency {
        phase: StartupInitializationPhase,
        service: StartupServiceType,
        dependency: StartupServiceType,
    },
    CircularDependency {
        phase: StartupInitializationPhase,
        services: ServiceTypeList,
    },
    ServiceFailures {
        failures: StartupInitializationFailures,
    },
    /// Options registered with `validate_on_start` failed
    /// ([`ServiceProvider::validate_options_on_start`]); no startup service was initialized.
    Options { failures: Vec<OptionsError> },
}

impl fmt::Display for StartupInitializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDependency {
                phase,
                service,
                dependency,
            } => write!(
                formatter,
                "{phase} startup service `{service}` depends on `{dependency}`, but no registered startup service matches that dependency"
            ),
            Self::CircularDependency { phase, services } => write!(
                formatter,
                "circular {phase} startup service dependency detected among: {services}"
            ),
            Self::ServiceFailures { failures } => {
                write!(formatter, "one or more startup services failed: {failures}")
            }
            Self::Options { failures } => {
                formatter.write_str("options failed on start: ")?;
                for (index, failure) in failures.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str("; ")?;
                    }
                    write!(formatter, "{failure}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for StartupInitializationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceTypeList(Vec<StartupServiceType>);

impl ServiceTypeList {
    fn new(mut services: Vec<StartupServiceType>) -> Self {
        services.sort_by(|left, right| left.name().cmp(right.name()));
        services.dedup();
        Self(services)
    }
}

impl fmt::Display for ServiceTypeList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = self
            .0
            .iter()
            .map(StartupServiceType::name)
            .collect::<Vec<_>>()
            .join(", ");
        formatter.write_str(&names)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupInitializationFailures(Vec<StartupInitializationFailure>);

impl StartupInitializationFailures {
    fn new(failures: Vec<StartupInitializationFailure>) -> Self {
        Self(failures)
    }

    pub fn as_slice(&self) -> &[StartupInitializationFailure] {
        &self.0
    }
}

impl fmt::Display for StartupInitializationFailures {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let messages = self
            .0
            .iter()
            .map(|failure| format!("{}: {}", failure.service, failure.message))
            .collect::<Vec<_>>()
            .join("; ");
        formatter.write_str(&messages)
    }
}

#[derive(Clone, Default)]
pub struct StartupRegistry {
    services: Vec<Arc<KeyedStartupService>>,
    factories: Vec<StartupRegistryFactory>,
    default_timeout: Option<Duration>,
}

impl StartupRegistry {
    /// The timeout of a service that sets none, unless the registry sets another.
    pub const DEFAULT_INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(30);

    pub fn new() -> Self {
        Self::default()
    }

    /// The timeout of a service whose [`AsyncInitializable::initialization_timeout`] is `None`.
    pub fn default_initialization_timeout(&self) -> Duration {
        self.default_timeout
            .unwrap_or(Self::DEFAULT_INITIALIZATION_TIMEOUT)
    }

    pub fn set_default_initialization_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.default_timeout = Some(timeout);
        self
    }

    /// Adds `service`, keyed by its [`StartupInitializable::startup_service_type`], else by its
    /// concrete type `T`.
    pub fn add<T>(&mut self, service: T) -> &mut Self
    where
        T: StartupInitializable,
    {
        self.push(KeyedStartupService::new(
            Arc::new(service),
            StartupServiceType::of::<T>(),
        ))
    }

    /// Adds the shared `service`, keyed by its [`StartupInitializable::startup_service_type`],
    /// else by `T` (the type it is registered as, `dyn X` for a trait object).
    pub fn add_shared<T>(&mut self, service: Arc<T>) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.push(KeyedStartupService::new(
            Arc::new(SharedStartupService { service }),
            StartupServiceType::of::<T>(),
        ))
    }

    /// Adds the shared `service` registered as `T` (a `dyn Trait` it implements, say), keyed by
    /// its [`StartupInitializable::startup_service_type`], else by `T`: what
    /// [`ServiceProvider::add_startup_singleton_for`] adds. Any startup service registered as `T`
    /// before is removed.
    pub fn add_shared_as<T, I>(&mut self, service: Arc<I>) -> &mut Self
    where
        T: ?Sized + 'static,
        I: StartupInitializable + ?Sized,
    {
        let registered_as = StartupServiceType::of::<T>();
        self.remove_registered(&registered_as);
        self.push(KeyedStartupService::new(
            Arc::new(SharedStartupService { service }),
            registered_as,
        ))
    }

    /// Adds the startup service `T`, resolved from the run's provider when startup runs and keyed
    /// by its [`StartupInitializable::startup_service_type`], else by `T`.
    pub fn add_resolved<T>(&mut self) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        self.add_resolved_with(StartupServiceType::of::<T>(), |services| {
            services
                .required_service::<T>()
                .map(|service| -> StartupService { Arc::new(SharedStartupService { service }) })
        })
    }

    /// Adds the startup service `resolve` gives from the run's provider when startup runs,
    /// registered as `service_type`.
    pub(crate) fn add_resolved_with(
        &mut self,
        service_type: StartupServiceType,
        resolve: impl Fn(&ServiceProvider) -> Result<StartupService, super::ServiceResolutionError>
        + Send
        + Sync
        + 'static,
    ) -> &mut Self {
        let key = service_type.clone();
        self.add_factory(
            service_type,
            move |context| {
                let resolved =
                    resolve(context.services()).map_err(|error| StartupInitializationFailure {
                        service: key.clone(),
                        message: error.to_string(),
                        timed_out: false,
                    });
                futures::future::ready(resolved).boxed_local()
            },
            false,
        )
    }

    /// Adds a startup service made by the asynchronous `factory` when startup runs, before any
    /// service is ordered or initialized. It is keyed by its own
    /// [`StartupInitializable::startup_service_type`], else by `service_type` (a Rust type or a
    /// runtime key), and ordered, initialized, timed and reported with every other startup
    /// service. A failing factory fails startup, reported under `service_type`; with a timer
    /// ([`Self::initialize_with_timer`]), so does one not done within the default timeout.
    pub fn add_resolved_async(
        &mut self,
        service_type: impl Into<StartupServiceType>,
        factory: impl Fn(&StartupInitializationContext) -> StartupServiceFuture + Send + Sync + 'static,
    ) -> &mut Self {
        self.add_factory(service_type.into(), factory, false)
    }

    /// [`Self::add_resolved_async`] for a service the other factories may read: every factory
    /// added this way is made (awaited together) before any other factory runs, so a factory
    /// that resolves the service and reads it finds it made. What a TS implementation of a
    /// contract is registered with (its object is made in JS's next turn): a Rust startup
    /// service's factory may read its state. When one fails, no other factory runs.
    pub fn add_resolved_async_first(
        &mut self,
        service_type: impl Into<StartupServiceType>,
        factory: impl Fn(&StartupInitializationContext) -> StartupServiceFuture + Send + Sync + 'static,
    ) -> &mut Self {
        self.add_factory(service_type.into(), factory, true)
    }

    pub fn with<T>(mut self, service: T) -> Self
    where
        T: StartupInitializable,
    {
        self.add(service);
        self
    }

    pub fn extend(&mut self, other: StartupRegistry) -> &mut Self {
        self.services.extend(other.services);
        self.factories.extend(other.factories);
        if other.default_timeout.is_some() {
            self.default_timeout = other.default_timeout;
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.services.is_empty() && self.factories.is_empty()
    }

    /// The services added already made (not those a factory makes when startup runs).
    pub fn services(&self) -> Vec<StartupService> {
        self.services.iter().map(Self::erased).collect()
    }

    /// Removes every startup service registered as `service_type` (added already made, or by a
    /// factory): what registering a service under that key does, so the last registration of a
    /// key wins.
    pub(crate) fn remove_registered(&mut self, service_type: &StartupServiceType) {
        self.services
            .retain(|service| service.registered_as != *service_type);
        self.factories
            .retain(|factory| factory.service_type != *service_type);
    }

    pub(crate) fn remove(&mut self, service_type: StartupServiceType) {
        self.services.retain(|service| service.key != service_type);
        self.factories
            .retain(|factory| factory.service_type != service_type);
    }

    pub(crate) fn contains(&self, service_type: StartupServiceType) -> bool {
        self.services
            .iter()
            .any(|service| service.key == service_type)
            || self
                .factories
                .iter()
                .any(|factory| factory.service_type == service_type)
    }

    fn push(&mut self, service: Arc<KeyedStartupService>) -> &mut Self {
        self.remove(service.key.clone());
        self.services.push(service);
        self
    }

    fn add_factory(
        &mut self,
        service_type: StartupServiceType,
        factory: impl Fn(&StartupInitializationContext) -> StartupServiceFuture + Send + Sync + 'static,
        first: bool,
    ) -> &mut Self {
        self.remove(service_type.clone());
        self.factories.push(StartupRegistryFactory {
            service_type,
            factory: Arc::new(factory),
            first,
        });
        self
    }

    fn erased(service: &Arc<KeyedStartupService>) -> StartupService {
        Arc::clone(service) as StartupService
    }

    /// Initializes every startup service, layer by layer in dependency order (a layer's services
    /// concurrently), stopping at the first layer with failures. Without a timer, nothing times
    /// out; see [`Self::initialize_with_timer`]. First it makes the options registered with
    /// `validate_on_start` ([`ServiceProvider::validate_options_on_start`]); when one fails,
    /// no service is initialized. Then it makes every startup service a factory makes (awaiting
    /// asynchronous factories); when one fails, no service is initialized.
    pub async fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> Result<(), StartupInitializationError> {
        self.run(context, None::<fn(Duration) -> std::future::Pending<()>>)
            .await
    }

    /// [`Self::initialize`], failing a service that takes longer than its timeout
    /// ([`AsyncInitializable::initialization_timeout`], else the registry's default). `timer`
    /// makes a future that completes after the given duration (the executor's timer: the core
    /// has none of its own).
    pub async fn initialize_with_timer<F>(
        &self,
        context: StartupInitializationContext,
        timer: impl Fn(Duration) -> F,
    ) -> Result<(), StartupInitializationError>
    where
        F: Future<Output = ()>,
    {
        self.run(context, Some(timer)).await
    }

    async fn run<F>(
        &self,
        context: StartupInitializationContext,
        timer: Option<impl Fn(Duration) -> F>,
    ) -> Result<(), StartupInitializationError>
    where
        F: Future<Output = ()>,
    {
        context
            .services()
            .validate_options_on_start()
            .map_err(|failures| StartupInitializationError::Options { failures })?;
        let default_timeout = self.default_initialization_timeout();
        let services = self
            .resolved_services(&context, timer.as_ref(), default_timeout)
            .await?;
        // A dependency no startup service is keyed by is met by a service registered under its
        // key that is no startup service (an app's own implementation of a trait): never by a
        // startup service registered under it with a key of its own.
        let provider = context.services().clone();
        let registered = |dependency: &StartupServiceType| {
            provider.contains_key(&dependency.key)
                && !services
                    .iter()
                    .any(|service| service.registered_as == *dependency)
        };
        for layer in Self::runtime_initialization_layers_for(&services, registered)? {
            let results = join_all(layer.into_iter().map(|service| {
                let context = context.clone();
                let deadline = timer.as_ref().map(|timer| {
                    let timeout = service.initialization_timeout().unwrap_or(default_timeout);
                    (timeout, Box::pin(timer(timeout)))
                });
                async move {
                    if service.is_initialized() {
                        return Ok(());
                    }
                    let service_type = service.key.clone();
                    let failed = |message: String, timed_out: bool| StartupInitializationFailure {
                        service: service_type,
                        message,
                        timed_out,
                    };
                    let initializing = service.initialize(context);
                    let result = match deadline {
                        None => initializing.await,
                        Some((timeout, expired)) => match select(initializing, expired).await {
                            Either::Left((result, _)) => result,
                            Either::Right(_) => {
                                return Err(failed(
                                    format!("did not initialize within {} ms", timeout.as_millis()),
                                    true,
                                ));
                            }
                        },
                    };
                    result.map_err(|error| failed(error.to_string(), false))
                }
            }))
            .await;

            let failures = results
                .into_iter()
                .filter_map(Result::err)
                .collect::<Vec<_>>();
            if !failures.is_empty() {
                return Err(StartupInitializationError::ServiceFailures {
                    failures: StartupInitializationFailures::new(failures),
                });
            }
        }

        Ok(())
    }

    /// The initialization layers of the services added already made (factories make theirs
    /// only when startup runs). It sees startup services only: a dependency no startup service
    /// is keyed by is reported missing here, though a startup run, which asks its provider,
    /// counts it met when a service that does not initialize is registered under its key.
    pub fn initialization_layers(
        &self,
    ) -> Result<Vec<Vec<StartupService>>, StartupInitializationError> {
        Ok(
            Self::runtime_initialization_layers_for(&self.services, |_| false)?
                .into_iter()
                .map(|layer| layer.iter().map(Self::erased).collect())
                .collect(),
        )
    }

    /// The runtime layers: a dependency no startup service matches is met when `registered`
    /// says a service of its key is registered (one that does not initialize, such as an app's
    /// own implementation of a trait a startup service depends on).
    fn runtime_initialization_layers_for(
        services: &[Arc<KeyedStartupService>],
        registered: impl Fn(&StartupServiceType) -> bool,
    ) -> Result<Vec<Vec<Arc<KeyedStartupService>>>, StartupInitializationError> {
        Self::initialization_layers_for(
            services,
            StartupInitializationPhase::Runtime,
            |service| service.initialization_depends_on(),
            registered,
        )
    }

    fn initialization_layers_for(
        services: &[Arc<KeyedStartupService>],
        phase: StartupInitializationPhase,
        dependencies_for: impl Fn(&KeyedStartupService) -> Vec<StartupServiceType>,
        registered: impl Fn(&StartupServiceType) -> bool,
    ) -> Result<Vec<Vec<Arc<KeyedStartupService>>>, StartupInitializationError> {
        let dependency_matches =
            Self::dependency_matches(services, phase, dependencies_for, registered)?;
        let mut initialized = HashSet::new();
        let mut remaining = (0..services.len()).collect::<Vec<_>>();
        let mut layers = Vec::new();

        while !remaining.is_empty() {
            let current_layer = remaining
                .iter()
                .copied()
                .filter(|index| {
                    dependency_matches[*index]
                        .iter()
                        .all(|dependency| initialized.contains(dependency))
                })
                .collect::<Vec<_>>();

            if current_layer.is_empty() {
                return Err(StartupInitializationError::CircularDependency {
                    phase,
                    services: ServiceTypeList::new(
                        remaining
                            .iter()
                            .map(|index| services[*index].key.clone())
                            .collect(),
                    ),
                });
            }

            for index in &current_layer {
                initialized.insert(*index);
            }

            remaining.retain(|index| !initialized.contains(index));
            layers.push(
                current_layer
                    .into_iter()
                    .map(|index| Arc::clone(&services[index]))
                    .collect(),
            );
        }

        Ok(layers)
    }

    fn dependency_matches(
        services: &[Arc<KeyedStartupService>],
        phase: StartupInitializationPhase,
        dependencies_for: impl Fn(&KeyedStartupService) -> Vec<StartupServiceType>,
        registered: impl Fn(&StartupServiceType) -> bool,
    ) -> Result<Vec<Vec<usize>>, StartupInitializationError> {
        let mut dependency_matches = Vec::with_capacity(services.len());
        for service in services {
            let mut service_dependencies = Vec::new();
            for dependency in dependencies_for(service) {
                let mut matches = services
                    .iter()
                    .enumerate()
                    .filter_map(|(index, candidate)| (candidate.key == dependency).then_some(index))
                    .collect::<Vec<_>>();

                if matches.is_empty() && !registered(&dependency) {
                    return Err(StartupInitializationError::MissingDependency {
                        phase,
                        service: service.key.clone(),
                        dependency,
                    });
                }

                service_dependencies.append(&mut matches);
            }

            service_dependencies.sort_unstable();
            service_dependencies.dedup();
            dependency_matches.push(service_dependencies);
        }

        Ok(dependency_matches)
    }

    /// The services added already made, then the ones every factory makes (awaited together,
    /// each within the default timeout when there is a timer): first those added with
    /// [`Self::add_resolved_async_first`], then the others; a made service replaces an earlier
    /// one of its key.
    async fn resolved_services<F>(
        &self,
        context: &StartupInitializationContext,
        timer: Option<&impl Fn(Duration) -> F>,
        default_timeout: Duration,
    ) -> Result<Vec<Arc<KeyedStartupService>>, StartupInitializationError>
    where
        F: Future<Output = ()>,
    {
        let mut services = self.services.clone();
        for first in [true, false] {
            let making: Vec<_> = self
                .factories
                .iter()
                .filter(|factory| factory.first == first)
                .map(|factory| Self::make(factory, context, timer, default_timeout))
                .collect();
            let made = join_all(making).await;
            let mut failures = Vec::new();
            for result in made {
                match result {
                    Ok(service) => {
                        services.retain(|existing| existing.key != service.key);
                        services.push(service);
                    }
                    Err(failure) => failures.push(failure),
                }
            }
            if !failures.is_empty() {
                return Err(StartupInitializationError::ServiceFailures {
                    failures: StartupInitializationFailures::new(failures),
                });
            }
        }
        Ok(services)
    }

    /// What `factory` makes (its factory called now, as every factory of a phase is before
    /// any is awaited), within the default timeout when there is a timer.
    fn make<F>(
        factory: &StartupRegistryFactory,
        context: &StartupInitializationContext,
        timer: Option<&impl Fn(Duration) -> F>,
        default_timeout: Duration,
    ) -> impl Future<Output = Result<Arc<KeyedStartupService>, StartupInitializationFailure>>
    where
        F: Future<Output = ()>,
    {
        let service_type = factory.service_type.clone();
        let making = (factory.factory)(context);
        let expired = timer.map(|timer| Box::pin(timer(default_timeout)));
        async move {
            let result = match expired {
                None => making.await,
                Some(expired) => match select(making, expired).await {
                    Either::Left((result, _)) => result,
                    Either::Right(_) => {
                        return Err(StartupInitializationFailure {
                            service: service_type,
                            message: format!(
                                "was not made within {} ms",
                                default_timeout.as_millis()
                            ),
                            timed_out: true,
                        });
                    }
                },
            };
            match result {
                Ok(service) => Ok(KeyedStartupService::new(service, service_type)),
                Err(failure) => Err(StartupInitializationFailure {
                    service: service_type,
                    ..failure
                }),
            }
        }
    }
}

impl fmt::Debug for StartupRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupRegistry")
            .field("services", &self.services.len())
            .field("factories", &self.factories.len())
            .finish()
    }
}
