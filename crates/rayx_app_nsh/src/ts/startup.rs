//! Startup services written in TS: the interfaces `AsyncInitializable` and
//! `StartupInitializable` (mirroring `rayx_app`'s traits), the classes
//! [`StartupInitializationState`] and [`StartupInitializationContext`], and the adapter that
//! runs a TS startup service in the one startup run with Rust's.
//!
//! The startup run is a core task, outside any call from JS, so every member of the interfaces
//! is an `async fn` Rust awaits (NSH posts the call to JS's next turn and settles a promise it
//! returns); their TS signatures keep plain results (`isInitialized(): boolean`). Once the run
//! has made a TS startup service, the adapter reads `startupServiceType`,
//! `initializationDependsOn` and `initializationTimeout` once and keeps them for the run's
//! ordering and timeout; it asks `isInitialized()` just before initializing, as the run asks a
//! Rust service. A timeout fails startup but does not stop the JS work.

use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::prelude::*;
use rayx_app::di::{
    self, StartupInitializationFailure, StartupInitializationResult, StartupService,
    StartupServiceError, StartupServiceFuture, StartupServiceType, ThreadBound,
};

use super::container::{self, SharedTsService};
use super::provider::{ServiceKeyArg, ServiceProvider, root_of};

/// A service written in TS that initializes asynchronously when the app starts: `start()`
/// awaits its `initialize` once, within its timeout, before the app is ready.
#[interface]
#[allow(async_fn_in_trait)]
pub trait AsyncInitializable {
    /// Whether it initialized already: asked just before `initialize`, which is skipped when it
    /// is.
    #[ts("(): boolean")]
    async fn is_initialized(&self) -> Result<bool, String>;

    /// Initializes it: startup waits for the promise, and fails with what it rejects with (or
    /// `initialize` throws).
    #[ts("(context: StartupInitializationContext): Promise<void>")]
    async fn initialize(
        &self,
        context: Instance<StartupInitializationContext>,
    ) -> Result<(), String>;

    /// How long, in milliseconds, `initialize` may take before startup fails; `undefined` (or
    /// no such member) takes the builder's default (`defaultInitializationTimeout`).
    #[ts("(): number | undefined")]
    async fn initialization_timeout(&self) -> Result<Option<f64>, String> {
        Ok(None)
    }
}

/// An [`AsyncInitializable`] that says what it is keyed by and what it waits for in the startup
/// run. It repeats `AsyncInitializable`'s members (an interface extends none), so a class
/// implementing it implements both.
#[interface]
#[allow(async_fn_in_trait)]
pub trait StartupInitializable {
    /// Whether it initialized already: asked just before `initialize`, which is skipped when it
    /// is.
    #[ts("(): boolean")]
    async fn is_initialized(&self) -> Result<bool, String>;

    /// Initializes it: startup waits for the promise, and fails with what it rejects with (or
    /// `initialize` throws).
    #[ts("(context: StartupInitializationContext): Promise<void>")]
    async fn initialize(
        &self,
        context: Instance<StartupInitializationContext>,
    ) -> Result<(), String>;

    /// How long, in milliseconds, `initialize` may take before startup fails; `undefined` (or
    /// no such member) takes the builder's default (`defaultInitializationTimeout`).
    #[ts("(): number | undefined")]
    async fn initialization_timeout(&self) -> Result<Option<f64>, String> {
        Ok(None)
    }

    /// The token other startup services name in `initializationDependsOn` to wait for this
    /// one; left out, it is keyed by the token it is registered under. An implementation of a
    /// contract stays keyed by its contract.
    #[ts("(): ServiceKey<unknown>")]
    async fn startup_service_type(&self) -> Result<Option<ServiceKeyArg>, String> {
        Ok(None)
    }

    /// The startup services (TS and Rust, by their tokens: `I18nService`, a TS class, a
    /// `ServiceToken`) that initialize before this one.
    #[ts("(): ServiceKey<unknown>[]")]
    async fn initialization_depends_on(&self) -> Result<Vec<ServiceKeyArg>, String> {
        Ok(Vec::new())
    }
}

/// Whether a startup service initialized, for its `isInitialized()`: `markInitialized()` once
/// it did (or when something else initialized it), so a second run skips it.
#[class]
pub struct StartupInitializationState {
    state: di::StartupInitializationState,
}

impl Default for StartupInitializationState {
    fn default() -> Self {
        Self::new()
    }
}

#[class]
impl StartupInitializationState {
    /// Not initialized yet.
    pub fn new() -> Self {
        Self {
            state: di::StartupInitializationState::default(),
        }
    }

    /// Whether it was marked initialized.
    pub fn is_initialized(&self) -> bool {
        self.state.is_initialized()
    }

    /// Marks it initialized.
    pub fn mark_initialized(&self) {
        self.state.mark_initialized();
    }
}

/// What a startup service's `initialize` gets: the app's root `ServiceProvider`, the one
/// `start()` resolves with.
#[class]
pub struct StartupInitializationContext {
    services: Instance<ServiceProvider>,
}

#[class]
impl StartupInitializationContext {
    /// The app's root provider.
    #[getter]
    pub fn services(&self) -> Instance<ServiceProvider> {
        self.services.clone()
    }
}

/// Adds `service` (registered in the provider already) to `provider`'s startup run, keyed
/// `registered_as` unless it names its own key: made by the run (its factory called in JS's
/// next turn unless it is made; then `made` runs, for a contract's adapter) and then
/// initialized as a [`JsStartupService`].
pub(crate) fn add(
    provider: &mut rayx_app::ServiceProvider,
    service: SharedTsService,
    registered_as: StartupServiceType,
    made: Option<fn(&rayx_app::ServiceProvider)>,
) {
    let key = registered_as.clone();
    provider.add_startup_resolved_async(registered_as, move |context| {
        let service = Arc::clone(&service);
        let root = context.services().clone();
        let key = key.clone();
        async move {
            let failed = |message: String| StartupInitializationFailure {
                service: key.clone(),
                message,
                timed_out: false,
            };
            let registration = service.get().map_err(|error| failed(error.to_string()))?;
            let object = container::make(&root, &registration)
                .await
                .map_err(failed)?;
            if let Some(made) = made {
                made(&root);
            }
            let adapter =
                JsStartupService::read(object, &registration, key.clone(), made.is_some())
                    .await
                    .map_err(failed)?;
            Ok(Arc::new(adapter) as StartupService)
        }
        .boxed_local() as StartupServiceFuture
    });
}

/// What a TS startup service's adapter keeps on the engine thread.
struct Standing {
    object: JsStartupInitializable,
    sys: System,
}

/// A TS startup service as the startup run sees it (`rayx_app`'s traits): its object, and
/// what it said of its key, dependencies and timeout when the run made it.
pub(crate) struct JsStartupService {
    standing: ThreadBound<Standing>,
    key: StartupServiceType,
    depends_on: Vec<StartupServiceType>,
    timeout: Option<Duration>,
}

impl JsStartupService {
    /// The adapter of `object`, registered as `registered_as`: its key, dependencies and
    /// timeout read once, now. A `contract`'s implementation keeps `registered_as` (the
    /// contract's key) whatever its `startupServiceType()` names.
    pub(crate) async fn read(
        object: JsHandle,
        registration: &container::TsService,
        registered_as: StartupServiceType,
        contract: bool,
    ) -> Result<Self, String> {
        let sys = registration.sys.clone();
        let object = JsStartupInitializable::new(object);
        // Read once all the same; a contract's implementation stays keyed by its contract
        // (`dyn Trait`), which Rust startup services depending on it name.
        let named = object.startup_service_type().await?;
        let key = match named {
            Some(token) if !contract => token.startup_type(&sys),
            _ => registered_as,
        };
        let depends_on = object
            .initialization_depends_on()
            .await?
            .iter()
            .map(|token| token.startup_type(&sys))
            .collect();
        let timeout = match object.initialization_timeout().await? {
            None => None,
            Some(ms) => Some(
                Duration::try_from_secs_f64(ms / 1000.0)
                    .map_err(|_| format!("initializationTimeout() gives milliseconds, not {ms}"))?,
            ),
        };
        let standing = ThreadBound::new(Standing { object, sys }, key.name().to_owned())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            standing,
            key,
            depends_on,
            timeout,
        })
    }
}

impl di::AsyncInitializable for JsStartupService {
    /// Never initialized here: [`Self::initialize`] asks the object just before it initializes.
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        context: di::StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let standing = self.standing.get();
        async move {
            let standing = standing.map_err(|error| StartupServiceError::new(error.to_string()))?;
            if standing
                .object
                .is_initialized()
                .await
                .map_err(StartupServiceError::new)?
            {
                return Ok(());
            }
            let context = Instance::new(StartupInitializationContext {
                services: root_of(&standing.sys, context.services().clone()),
            });
            standing
                .object
                .initialize(context)
                .await
                .map_err(StartupServiceError::new)
        }
        .boxed_local()
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        self.timeout
    }
}

impl di::StartupInitializable for JsStartupService {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(self.key.clone())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        self.depends_on.clone()
    }
}
