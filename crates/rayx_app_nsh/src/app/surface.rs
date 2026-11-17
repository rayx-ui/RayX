//! The root provider of an engine surface, and the app's startup.

use std::{fmt, future::Future, rc::Rc, time::Duration};

use futures::future::{FutureExt, LocalBoxFuture, Shared};

use crate::NshServicesAppBuilderExt as _;
use dxn_reactive::ReactiveError;
use nsh_core::prelude::*;
use rayx_app::AppBuilder as _;
use rayx_app::{AppBuildError, ServiceValidationError};
use rayx_app::{
    ConfigurationError, RayXAppBuilder, ReadyError, ReadyHooks, ServiceProvider,
    ServiceResolutionError,
    di::{StartupInitializationContext, StartupInitializationError},
};

/// Why the app has no root provider on a surface, or a service could not be resolved from it.
///
/// It is `#[non_exhaustive]`: a later RayX may add a variant without a breaking change, so a
/// `match` over it outside `rayx_app_nsh` has a wildcard arm:
///
/// ```
/// use rayx_app_nsh::AppError;
///
/// fn waiting(error: &AppError) -> bool {
///     match error {
///         AppError::NotStarted | AppError::Starting => true,
///         _ => false,
///     }
/// }
/// # assert!(waiting(&AppError::Starting));
/// ```
///
/// A `match` that names every variant and no wildcard does not compile:
///
/// ```compile_fail
/// use rayx_app_nsh::AppError;
///
/// fn waiting(error: &AppError) -> bool {
///     match error {
///         AppError::NotStarted | AppError::Starting => true,
///         AppError::BuilderStarted
///         | AppError::Configuration(_)
///         | AppError::Startup(_)
///         | AppError::Ready(_)
///         | AppError::Resolution(_)
///         | AppError::Activation(_)
///         | AppError::Failed(_) => false,
///     }
/// }
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum AppError {
    /// The app has not started on this surface (a Rust root's
    /// [`StartAppBuilderExt::start`], TSX's `await builder.start()`).
    NotStarted,
    /// The builder started already (TSX: a `RayXAppBuilder` after its `start()`): make a new
    /// one.
    BuilderStarted,
    /// The app is starting on this surface; wait for [`app_state`] to change.
    Starting,
    /// The app's registrations are not complete: a service the app, its host or a registration
    /// requires is missing, or a singleton depends on a scoped service. Nothing was made.
    Services(ServiceValidationError),
    /// The app's configuration did not load.
    Configuration(ConfigurationError),
    /// Options validated on start failed, a startup service failed or timed out, or startup
    /// could not be ordered (a cycle, a missing dependency). The app does not start.
    Startup(StartupInitializationError),
    /// A ready hook failed ([`rayx_app::ReadyAppBuilderExt::on_ready`]). The app does not
    /// start.
    Ready(ReadyError),
    /// A service did not resolve.
    Resolution(ServiceResolutionError),
    /// A view model's activation failed.
    Activation(ReactiveError),
    /// Startup failed, as seen by a caller that joined a startup another one began (the
    /// message is the startup error's).
    Failed(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotStarted => f.write_str(
                "the app has not started on this surface: start its RayXAppBuilder (`builder.start()`)",
            ),
            Self::BuilderStarted => {
                f.write_str("the RayXAppBuilder has started already: make a new one")
            }
            Self::Starting => f.write_str("the app is still starting on this surface"),
            Self::Services(error) => error.fmt(f),
            Self::Configuration(error) => {
                write!(f, "the app's configuration did not load: {error}")
            }
            Self::Startup(error) => write!(f, "the app did not start: {error}"),
            Self::Ready(error) => write!(f, "a ready hook failed: {error}"),
            Self::Resolution(error) => write!(f, "{error}"),
            Self::Activation(error) => write!(f, "the view model did not activate: {error}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for AppError {}

impl From<ServiceValidationError> for AppError {
    fn from(error: ServiceValidationError) -> Self {
        Self::Services(error)
    }
}

impl From<AppBuildError> for AppError {
    fn from(error: AppBuildError) -> Self {
        match error {
            AppBuildError::Services(error) => Self::Services(error),
            AppBuildError::Configuration(error) => Self::Configuration(error),
        }
    }
}

impl From<ConfigurationError> for AppError {
    fn from(error: ConfigurationError) -> Self {
        Self::Configuration(error)
    }
}

impl From<StartupInitializationError> for AppError {
    fn from(error: StartupInitializationError) -> Self {
        Self::Startup(error)
    }
}

impl From<ReadyError> for AppError {
    fn from(error: ReadyError) -> Self {
        Self::Ready(error)
    }
}

impl From<ReactiveError> for AppError {
    fn from(error: ReactiveError) -> Self {
        Self::Activation(error)
    }
}

impl From<ServiceResolutionError> for AppError {
    fn from(error: ServiceResolutionError) -> Self {
        Self::Resolution(error)
    }
}

/// Where the app is on a surface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AppState {
    #[default]
    NotStarted,
    /// Startup services are initializing.
    Starting,
    /// Every startup service initialized: services resolve.
    Ready,
    /// Startup failed; the message names the services that failed or timed out.
    Failed(String),
}

/// A startup in progress, shared by every caller that waits for it.
type Startup = Shared<LocalBoxFuture<'static, Result<ServiceProvider, Rc<AppError>>>>;

/// The surface's slot (`global` makes one per surface).
#[derive(Default)]
struct SurfaceApp {
    state: AppState,
    root: Option<ServiceProvider>,
    /// The startup while it runs: whoever polls it drives it, so a caller that is cancelled
    /// (its task's owner went) leaves it to the next one.
    starting: Option<Startup>,
}

/// Where the app is on this surface. Reading it subscribes, so a Rust root can show a splash
/// while it is [`AppState::Starting`] and the app once it is [`AppState::Ready`].
pub fn app_state() -> AppState {
    global::<SurfaceApp>().with(|app| app.state.clone())
}

/// Installs `provider` as this surface's root, ready, replacing any earlier one. Its startup
/// services count as initialized: use it for a provider whose startup ran elsewhere, or that
/// has none. Call it on the engine's thread, from the app's code.
pub fn install(provider: ServiceProvider) {
    global::<SurfaceApp>().set(SurfaceApp {
        state: AppState::Ready,
        root: Some(provider),
        starting: None,
    });
}

/// This surface's root provider once the app is ready, read so that the caller follows the
/// app's start (as [`app_state`] is); `None` before, and outside an engine.
pub(crate) fn tracked_root() -> Option<ServiceProvider> {
    try_global::<SurfaceApp>()?.with(|app| match (&app.state, &app.root) {
        (AppState::Ready, Some(root)) => Some(root.clone()),
        _ => None,
    })
}

/// This surface's root provider, once the app has started on it.
pub fn app_services() -> Result<ServiceProvider, AppError> {
    global::<SurfaceApp>().with_untracked(|app| match (&app.state, &app.root) {
        (AppState::Ready, Some(root)) => Ok(root.clone()),
        (AppState::Starting, _) => Err(AppError::Starting),
        _ => Err(AppError::NotStarted),
    })
}

/// Starts the app on this surface with `builder`: builds the root provider (NSH's services added
/// where the app registered none, [`NshServicesAppBuilderExt::add_nsh_services`]) and
/// initializes every startup service in dependency order (a layer's services concurrently),
/// each within its timeout ([`AsyncInitializable::initialization_timeout`], else the builder's
/// default). The root is installed, and [`app_state`] becomes `Ready`, only once all of them
/// initialized. One failure or timeout fails startup ([`AppState::Failed`]); a later call tries
/// again. Once started, it returns the root at once and drops `builder`.
///
/// Calls while it starts wait for the same startup and get its result (their builders are
/// dropped). The startup belongs to the surface, not to the caller: when the task that began it
/// is cancelled, the next call (or another waiting caller) carries it on, so the state never
/// stays `Starting`. With no builder (a TS builder started already) and nothing started or
/// starting, it fails with [`AppError::BuilderStarted`].
///
/// Run it in an NSH task (`spawn`, or a class's `async` member): it waits on NSH's timers.
///
/// [`AsyncInitializable::initialization_timeout`]: rayx_app::di::AsyncInitializable::initialization_timeout
/// [`NshServicesAppBuilderExt::add_nsh_services`]: crate::NshServicesAppBuilderExt::add_nsh_services
pub(crate) async fn start_app_with(
    builder: Option<RayXAppBuilder>,
) -> Result<ServiceProvider, AppError> {
    crate::devtools::register();
    let slot = global::<SurfaceApp>();
    let running = slot.with_untracked(|app| match (&app.state, &app.root) {
        (AppState::Ready, Some(root)) => Ok(Ok(root.clone())),
        _ => Err(app.starting.clone()),
    });
    let startup = match running {
        Ok(ready) => return ready,
        Err(Some(startup)) => startup,
        Err(None) => {
            let builder = builder.ok_or(AppError::BuilderStarted)?;
            let startup: Startup = async move {
                let started = initialize(builder).await;
                global::<SurfaceApp>().set(match &started {
                    Ok(root) => SurfaceApp {
                        state: AppState::Ready,
                        root: Some(root.clone()),
                        starting: None,
                    },
                    Err(error) => SurfaceApp {
                        state: AppState::Failed(error.to_string()),
                        root: None,
                        starting: None,
                    },
                });
                started.map_err(Rc::new)
            }
            .boxed_local()
            .shared();
            slot.set(SurfaceApp {
                state: AppState::Starting,
                root: None,
                starting: Some(startup.clone()),
            });
            startup
        }
    };
    startup.await.map_err(|error| {
        Rc::try_unwrap(error).unwrap_or_else(|shared| AppError::Failed(shared.to_string()))
    })
}

/// Starts the app from a Rust root: `RayXAppBuilder::new()`, its chain, then `.start()`.
pub trait StartAppBuilderExt: Sized {
    /// Starts the app on this surface in a task of its own; watch [`app_state`]. Call it from
    /// the root component: the task ends with the owner it was started under. On a surface
    /// whose app started (or is starting), it keeps that app and drops this builder.
    fn start(self);

    /// Starts the app on this surface and gives its root provider, or why it did not start
    /// ([`AppError`]); on a surface whose app started (or is starting), that app's root. Await
    /// it in an NSH task: it waits on NSH's timers.
    fn start_async(self) -> impl Future<Output = Result<ServiceProvider, AppError>>;
}

impl StartAppBuilderExt for RayXAppBuilder {
    fn start(self) {
        // The task runs on its own (dropping its handle does not cancel it); it ends with the
        // current owner, as every task does.
        drop(spawn(async move {
            if start_app_with(Some(self)).await.is_ok() {
                crate::devtools::app_ready(None);
            }
        }));
    }

    fn start_async(self) -> impl Future<Output = Result<ServiceProvider, AppError>> {
        start_app_with(Some(self))
    }
}

async fn initialize(builder: RayXAppBuilder) -> Result<ServiceProvider, AppError> {
    crate::binding::start_dispatcher();
    let mut builder = builder.add_nsh_services();
    if let Some(runner) = crate::binding::EngineRunner::current() {
        builder.services_mut().replace_service(runner);
    }
    let root = builder.build_async().await?;
    crate::services::platform::sync::seed_platform(&root);
    let registry = root.startup_registry().clone();
    registry
        .initialize_with_timer(
            StartupInitializationContext::new(root.clone()),
            |timeout: Duration| timer(timeout),
        )
        .await?;
    crate::services::i18n::locale::keep_nsh_locale(&root);
    crate::services::i18n::translate::follow_i18n_changes(&root);
    // On the engine's thread, before the app counts as ready.
    if let Some(hooks) = root.service::<ReadyHooks>() {
        hooks.run(&root).await?;
    }
    Ok(root)
}
