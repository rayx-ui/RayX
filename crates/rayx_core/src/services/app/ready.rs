//! Ready hooks (RayX's `on_ready` / `on_ready_when`): code the app runs once its startup
//! services initialized, before it counts as ready.

use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;

use super::AppBuilder;
use crate::di::{AppService, ServiceProvider, ServiceResolutionError};
use crate::options::{Options, OptionsError};

/// What a ready hook gets: the app's root provider, its startup services initialized.
#[derive(Clone, Debug)]
pub struct ReadyContext {
    services: ServiceProvider,
}

impl ReadyContext {
    pub fn new(services: ServiceProvider) -> Self {
        Self { services }
    }

    /// The app's root provider.
    pub fn services(&self) -> &ServiceProvider {
        &self.services
    }

    /// The options `T` ([`ServiceProvider::options`]).
    pub fn options<T>(&self) -> Result<Options<T>, OptionsError>
    where
        T: Default + Send + Sync + 'static,
    {
        self.services.options::<T>()
    }
}

/// Why a ready hook failed: the app does not become ready.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyError {
    message: String,
}

impl ReadyError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ReadyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReadyError {}

impl From<String> for ReadyError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for ReadyError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<ServiceResolutionError> for ReadyError {
    fn from(error: ServiceResolutionError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<OptionsError> for ReadyError {
    fn from(error: OptionsError) -> Self {
        Self::new(error.to_string())
    }
}

/// What a ready hook's future gives.
pub type ReadyResult = Result<(), ReadyError>;

type ReadyHook = Arc<dyn Fn(ReadyContext) -> LocalBoxFuture<'static, ReadyResult> + Send + Sync>;
type ReadyPredicate = Arc<dyn Fn(&ServiceProvider) -> bool + Send + Sync>;

struct Registration {
    predicate: Option<ReadyPredicate>,
    hook: ReadyHook,
}

/// The app's ready hooks, in the order they were added ([`ReadyAppBuilderExt::on_ready`]). The
/// host runs them ([`ReadyHooks::run`]) once the startup services initialized, before the app
/// counts as ready; on NSH, `rayx_app`'s startup does.
#[derive(Clone, Default, AppService)]
pub struct ReadyHooks {
    registrations: Arc<Mutex<Vec<Registration>>>,
}

impl ReadyHooks {
    /// Adds `hook`, run after the hooks added before it.
    pub fn add<F, Fut>(&self, hook: F)
    where
        F: Fn(ReadyContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadyResult> + 'static,
    {
        self.push(None, hook);
    }

    /// Adds `hook`, run only when `predicate` holds for the app's root provider then.
    pub fn add_when<P, F, Fut>(&self, predicate: P, hook: F)
    where
        P: Fn(&ServiceProvider) -> bool + Send + Sync + 'static,
        F: Fn(ReadyContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadyResult> + 'static,
    {
        self.push(Some(Arc::new(predicate)), hook);
    }

    /// How many hooks there are.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Runs the hooks one after another, in order, with `services`; one whose predicate does
    /// not hold is skipped. The first that fails stops the others and fails the run.
    pub async fn run(&self, services: &ServiceProvider) -> ReadyResult {
        let hooks: Vec<_> = self
            .lock()
            .iter()
            .map(|registration| (registration.predicate.clone(), registration.hook.clone()))
            .collect();
        for (predicate, hook) in hooks {
            if predicate.is_none_or(|holds| holds(services)) {
                hook(ReadyContext::new(services.clone())).await?;
            }
        }
        Ok(())
    }

    fn push<F, Fut>(&self, predicate: Option<ReadyPredicate>, hook: F)
    where
        F: Fn(ReadyContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadyResult> + 'static,
    {
        self.lock().push(Registration {
            predicate,
            hook: Arc::new(move |context| hook(context).boxed_local()),
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Registration>> {
        self.registrations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for ReadyHooks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadyHooks")
            .field("hooks", &self.len())
            .finish()
    }
}

/// Ready hooks on the app builder (RayX's `RayXApp::on_ready` / `on_ready_when`).
pub trait ReadyAppBuilderExt: AppBuilder {
    /// Runs `hook` once the app's startup services initialized, before the app counts as
    /// ready; hooks run in the order they were added, and one that fails fails startup.
    fn on_ready<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(ReadyContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadyResult> + 'static,
    {
        ready_hooks(&mut self).add(hook);
        self
    }

    /// [`Self::on_ready`], run only when `predicate` holds for the app's root provider then
    /// (`rayx_runtime`'s `on_ready_when_platform` asks the platform service).
    fn on_ready_when<P, F, Fut>(mut self, predicate: P, hook: F) -> Self
    where
        P: Fn(&ServiceProvider) -> bool + Send + Sync + 'static,
        F: Fn(ReadyContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ReadyResult> + 'static,
    {
        ready_hooks(&mut self).add_when(predicate, hook);
        self
    }
}

impl<T> ReadyAppBuilderExt for T where T: AppBuilder {}

/// The builder's [`ReadyHooks`], registered the first time.
fn ready_hooks(builder: &mut impl AppBuilder) -> Arc<ReadyHooks> {
    let services = builder.services_mut();
    services.try_add_service(ReadyHooks::default());
    services
        .service::<ReadyHooks>()
        .unwrap_or_else(|| Arc::new(ReadyHooks::default()))
}
