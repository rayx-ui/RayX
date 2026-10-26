use super::provider::ServiceProvider;
use super::ready::DiReady;
use crate::options::OptionsError;
use futures::future::{Either, LocalBoxFuture, join_all, select};
use std::{
    any::{TypeId, type_name},
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

#[derive(Clone, Copy)]
pub struct StartupServiceType {
    id: TypeId,
    name: &'static str,
}

impl StartupServiceType {
    pub fn of<T>() -> Self
    where
        T: ?Sized + 'static,
    {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl fmt::Debug for StartupServiceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupServiceType")
            .field("name", &self.name)
            .field("id", &self.id)
            .finish()
    }
}

impl fmt::Display for StartupServiceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name)
    }
}

impl PartialEq for StartupServiceType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for StartupServiceType {}

impl Hash for StartupServiceType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
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
    fn startup_service_type(&self) -> StartupServiceType;

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
    fn startup_service_type(&self) -> StartupServiceType {
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

type StartupService = Arc<dyn StartupInitializable>;
type StartupServiceFactory = Arc<
    dyn Fn(&StartupInitializationContext) -> Result<StartupService, StartupInitializationFailure>
        + Send
        + Sync,
>;

#[derive(Clone)]
struct StartupRegistryFactory {
    service_type: StartupServiceType,
    factory: StartupServiceFactory,
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
    fn startup_service_type(&self) -> StartupServiceType {
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
    services: Vec<StartupService>,
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

    pub fn add<T>(&mut self, service: T) -> &mut Self
    where
        T: StartupInitializable,
    {
        let service: StartupService = Arc::new(service);
        let service_type = service.startup_service_type();
        self.remove(service_type);
        self.services.push(service);
        self
    }

    pub fn add_shared<T>(&mut self, service: Arc<T>) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        let service_type = service.startup_service_type();
        self.remove(service_type);
        self.services
            .push(Arc::new(SharedStartupService { service }));
        self
    }

    pub fn add_resolved<T>(&mut self) -> &mut Self
    where
        T: StartupInitializable + ?Sized,
    {
        let service_type = StartupServiceType::of::<T>();
        self.remove(service_type);
        self.factories.push(StartupRegistryFactory {
            service_type,
            factory: Arc::new(move |context| {
                let service = context
                    .services()
                    .required_service::<T>()
                    .map_err(|error| StartupInitializationFailure {
                        service: service_type,
                        message: error.to_string(),
                        timed_out: false,
                    })?;
                let startup_service: StartupService = Arc::new(SharedStartupService { service });
                Ok(startup_service)
            }),
        });
        self
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

    pub fn services(&self) -> &[StartupService] {
        &self.services
    }

    pub(crate) fn remove(&mut self, service_type: StartupServiceType) {
        self.services
            .retain(|service| service.startup_service_type() != service_type);
        self.factories
            .retain(|factory| factory.service_type != service_type);
    }

    pub(crate) fn contains(&self, service_type: StartupServiceType) -> bool {
        self.services
            .iter()
            .any(|service| service.startup_service_type() == service_type)
            || self
                .factories
                .iter()
                .any(|factory| factory.service_type == service_type)
    }

    /// Initializes every startup service, layer by layer in dependency order (a layer's services
    /// concurrently), stopping at the first layer with failures. Without a timer, nothing times
    /// out; see [`Self::initialize_with_timer`]. First it makes the options registered with
    /// `validate_on_start` ([`ServiceProvider::validate_options_on_start`]); when one fails,
    /// no service is initialized.
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
        let services = self.resolved_services(&context)?;
        let default_timeout = self.default_initialization_timeout();
        for layer in Self::runtime_initialization_layers_for(&services)? {
            let results = join_all(layer.into_iter().map(|service| {
                let context = context.clone();
                let deadline = timer.as_ref().map(|timer| {
                    let timeout = service.initialization_timeout().unwrap_or(default_timeout);
                    (timeout, Box::pin(timer(timeout)))
                });
                async move {
                    let service_type = service.startup_service_type();
                    if service.is_initialized() {
                        return Ok(());
                    }
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

    pub fn initialization_layers(
        &self,
    ) -> Result<Vec<Vec<StartupService>>, StartupInitializationError> {
        Self::runtime_initialization_layers_for(&self.services)
    }

    fn runtime_initialization_layers_for(
        services: &[StartupService],
    ) -> Result<Vec<Vec<StartupService>>, StartupInitializationError> {
        Self::initialization_layers_for(services, StartupInitializationPhase::Runtime, |service| {
            service.initialization_depends_on()
        })
    }

    fn initialization_layers_for(
        services: &[StartupService],
        phase: StartupInitializationPhase,
        dependencies_for: impl Fn(&StartupService) -> Vec<StartupServiceType>,
    ) -> Result<Vec<Vec<StartupService>>, StartupInitializationError> {
        let dependency_matches = Self::dependency_matches(services, phase, dependencies_for)?;
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
                            .map(|index| services[*index].startup_service_type())
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
        services: &[StartupService],
        phase: StartupInitializationPhase,
        dependencies_for: impl Fn(&StartupService) -> Vec<StartupServiceType>,
    ) -> Result<Vec<Vec<usize>>, StartupInitializationError> {
        let mut dependency_matches = Vec::with_capacity(services.len());
        for service in services {
            let mut service_dependencies = Vec::new();
            for dependency in dependencies_for(service) {
                let mut matches = services
                    .iter()
                    .enumerate()
                    .filter_map(|(index, candidate)| {
                        (candidate.startup_service_type() == dependency).then_some(index)
                    })
                    .collect::<Vec<_>>();

                if matches.is_empty() {
                    return Err(StartupInitializationError::MissingDependency {
                        phase,
                        service: service.startup_service_type(),
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

    fn resolved_services(
        &self,
        context: &StartupInitializationContext,
    ) -> Result<Vec<StartupService>, StartupInitializationError> {
        let mut services = self.services.clone();
        let mut failures = Vec::new();

        for factory in &self.factories {
            match (factory.factory)(context) {
                Ok(service) => {
                    let service_type = service.startup_service_type();
                    services.retain(|existing| existing.startup_service_type() != service_type);
                    services.push(service);
                }
                Err(failure) => failures.push(failure),
            }
        }

        if failures.is_empty() {
            Ok(services)
        } else {
            Err(StartupInitializationError::ServiceFailures {
                failures: StartupInitializationFailures::new(failures),
            })
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

pub trait ShutdownService: Send + Sync + 'static {
    fn shutdown(&self, services: &ServiceProvider) -> Result<(), String>;
}
