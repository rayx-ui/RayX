//! The root provider of an engine surface, and the app's startup.

use std::{fmt, rc::Rc, sync::OnceLock, time::Duration};

use futures::future::{FutureExt, LocalBoxFuture, Shared};

use dxn_reactive::ReactiveError;
use nsh_core::prelude::*;
use rayx_core::{
    ConfigurationError, RayXAppBuilder, ServiceProvider, ServiceResolutionError,
    di::{StartupInitializationContext, StartupInitializationError},
};

/// Why the app has no root provider on a surface, or a service could not be resolved from it.
#[derive(Debug)]
pub enum AppError {
    /// [`set_app`] named no builder.
    NoApp,
    /// The app has not started on this surface ([`start_app`], `AppServices.load()`).
    NotStarted,
    /// The app is starting on this surface; wait for [`app_state`] to change.
    Starting,
    /// The app's configuration did not load.
    Configuration(ConfigurationError),
    /// Options validated on start failed, a startup service failed or timed out, or startup
    /// could not be ordered (a cycle, a missing dependency). The app does not start.
    Startup(StartupInitializationError),
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
            Self::NoApp => f.write_str("no app: call rayx_app::set_app with the app's builder"),
            Self::NotStarted => f.write_str(
                "the app has not started on this surface: await AppServices.load() (TSX) or call rayx_app::start (Rust)",
            ),
            Self::Starting => f.write_str("the app is still starting on this surface"),
            Self::Configuration(error) => {
                write!(f, "the app's configuration did not load: {error}")
            }
            Self::Startup(error) => write!(f, "the app did not start: {error}"),
            Self::Resolution(error) => write!(f, "{error}"),
            Self::Activation(error) => write!(f, "the view model did not activate: {error}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for AppError {}

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

/// The app's builder function: a plain function, so every surface builds its own provider.
type BuildApp = fn(RayXAppBuilder) -> RayXAppBuilder;

static APP: OnceLock<BuildApp> = OnceLock::new();

/// Names the function every surface builds its root provider with when the app starts on it.
/// The first call wins (an app has one composition root); later calls are ignored and return
/// `false`.
pub fn set_app(build: BuildApp) -> bool {
    APP.set(build).is_ok()
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

/// This surface's root provider, once the app has started on it.
pub fn app_services() -> Result<ServiceProvider, AppError> {
    global::<SurfaceApp>().with_untracked(|app| match (&app.state, &app.root) {
        (AppState::Ready, Some(root)) => Ok(root.clone()),
        (AppState::Starting, _) => Err(AppError::Starting),
        _ => Err(AppError::NotStarted),
    })
}

/// Starts the app on this surface: builds the root provider with [`set_app`]'s function and
/// initializes every startup service in dependency order (a layer's services concurrently),
/// each within its timeout ([`AsyncInitializable::initialization_timeout`], else the builder's
/// default). The root is installed, and [`app_state`] becomes `Ready`, only once all of them
/// initialized. One failure or timeout fails startup ([`AppState::Failed`]); a later call tries
/// again. Once started, it returns the root at once.
///
/// Calls while it starts wait for the same startup and get its result. The startup belongs to
/// the surface, not to the caller: when the task that began it is cancelled, the next call (or
/// another waiting caller) carries it on, so the state never stays `Starting`.
///
/// Run it in an NSH task (`spawn`, or a class's `async` member): it waits on NSH's timers.
///
/// [`AsyncInitializable::initialization_timeout`]: rayx_core::di::AsyncInitializable::initialization_timeout
pub async fn start_app() -> Result<ServiceProvider, AppError> {
    let slot = global::<SurfaceApp>();
    let running = slot.with_untracked(|app| match (&app.state, &app.root) {
        (AppState::Ready, Some(root)) => Ok(Ok(root.clone())),
        _ => Err(app.starting.clone()),
    });
    let startup = match running {
        Ok(ready) => return ready,
        Err(Some(startup)) => startup,
        Err(None) => {
            let build = *APP.get().ok_or(AppError::NoApp)?;
            let startup: Startup = async move {
                let started = initialize(build).await;
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

/// [`start_app`] in a task of its own, for a Rust root: watch [`app_state`]. Call it from the
/// root component: the task ends with the owner it was started under.
pub fn start() {
    // The task runs on its own (dropping its handle does not cancel it); it ends with the
    // current owner, as every task does.
    drop(spawn(async {
        let _ = start_app().await;
    }));
}

async fn initialize(build: BuildApp) -> Result<ServiceProvider, AppError> {
    let root = build(RayXAppBuilder::new()).build_async().await?;
    let registry = root.startup_registry().clone();
    registry
        .initialize_with_timer(
            StartupInitializationContext::new(root.clone()),
            |timeout: Duration| timer(timeout),
        )
        .await?;
    Ok(root)
}
