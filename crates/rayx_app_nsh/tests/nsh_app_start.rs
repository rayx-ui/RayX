//! A Rust root's start without the old setter, through NSH's engine, headless: the root builds
//! `RayXAppBuilder::new()` with its chain and calls `.start()` (watching `app_state()` move
//! from `NotStarted` through `Starting` to `Ready` or `Failed`) or awaits `.start_async()`
//! (the root provider or the `AppError`); a second start on a started surface keeps the first
//! root, and two surfaces build two providers.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::element::Root;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app_nsh::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState,
};
use rayx_app_nsh::{
    AppError, AppService, AppState, RayXAppBuilder, ServiceAppBuilderExt as _, ServiceProvider,
    StartAppBuilderExt as _, app_services, app_state,
};

static NEXT: AtomicUsize = AtomicUsize::new(1);

/// A singleton: one per surface.
#[derive(AppService)]
struct Session {
    id: usize,
}

/// A startup service that initializes after 20 ms, or fails.
#[derive(Clone, AppService)]
#[app_service(startup)]
struct Catalog {
    fail: bool,
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Catalog {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let (fail, initialized) = (self.fail, self.initialized.clone());
        async move {
            timer(Duration::from_millis(20)).await;
            if fail {
                return Err("catalog unavailable".into());
            }
            initialized.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Catalog {}

/// The app's chain: a session, and a catalog that fails when `fail`.
fn app(fail: bool) -> RayXAppBuilder {
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    MADE.with_borrow_mut(|made| made.push(id));
    ::rayx_app::testing::add_host_fakes(RayXAppBuilder::new())
        .add_singleton(Session { id })
        .add_startup_singleton(Catalog {
            fail,
            initialized: StartupInitializationState::default(),
        })
}

thread_local! {
    /// How the root starts the app.
    static HOW: RefCell<How> = const { RefCell::new(How::Start { fail: false, twice: false }) };
    /// What `start_async()` gave, as text.
    static ASYNC: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The sessions of the builders made, in order.
    static MADE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Whether `AgainWhenReady` started again.
    /// The surface root's session once `AgainWhenReady` started again.
    static AGAIN: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    /// The states the root saw, in order.
    static STATES: RefCell<Vec<AppState>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy)]
enum How {
    /// `app(fail).start()`, and a second builder's `start()` after it when `twice`.
    Start { fail: bool, twice: bool },
    /// `app(fail).start_async()` in a task of the root's.
    StartAsync { fail: bool },
    /// `app(false).start()`, then, once the app is ready, another builder's `start()` and a
    /// third's `start_async()`.
    AgainWhenReady,
}

/// The session of `root`.
fn session(root: &ServiceProvider) -> usize {
    root.required_service::<Session>().unwrap().id
}

/// A Rust root: starts the app, and shows its state and its session.
#[component]
fn Start() -> impl IntoElement {
    match HOW.with_borrow(|how| *how) {
        How::Start { fail, twice } => {
            app(fail).start();
            if twice {
                app(false).start();
            }
        }
        How::AgainWhenReady => {
            app(false).start();
            spawn(async move {
                while app_services().is_err() {
                    timer(Duration::from_millis(2)).await;
                }
                app(false).start();
                let again = app(false).start_async().await.unwrap();
                ASYNC.with_borrow_mut(|kept| *kept = Some(format!("root {}", session(&again))));
                // Later still, the surface's root.
                timer(Duration::from_millis(20)).await;
                let root = session(&app_services().unwrap());
                AGAIN.with(|again| again.set(Some(root)));
            });
        }
        How::StartAsync { fail } => {
            spawn(async move {
                let started = app(fail).start_async().await;
                let shown = match started {
                    Ok(root) => format!("root {}", session(&root)),
                    Err(error) => format!("error {error}"),
                };
                ASYNC.with_borrow_mut(|kept| *kept = Some(shown));
            });
        }
    }
    dynamic(move || {
        let state = app_state();
        STATES.with_borrow_mut(|states| states.push(state.clone()));
        match state {
            AppState::Ready => text(format!("ready {}", session(&app_services().unwrap()))),
            AppState::Failed(message) => text(format!("failed: {message}")),
            state => text(format!("{state:?}")),
        }
        .into_element()
    })
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// A surface running `Start` as `how` says.
fn surface(how: How) -> (LocalEngine<ApproxTextEngine>, Receiver<MountBatch>) {
    HOW.with_borrow_mut(|kept| *kept = how);
    STATES.with_borrow_mut(Vec::clear);
    MADE.with_borrow_mut(Vec::clear);
    AGAIN.with(|again| again.set(None));
    ASYNC.with_borrow_mut(|kept| kept.take());
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = LocalEngine::start(
        EngineConfig::new(App::Rust(Root::of::<Start>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    (engine, rx)
}

/// Pumps `engine` until a text starting with `prefix` shows (or 10 s): that text.
fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    prefix: &str,
) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item
                    && text.text.starts_with(prefix)
                {
                    return text.text.clone();
                }
            }
        }
    }
    panic!("no text starting with {prefix:?}");
}

#[test]
fn a_rust_root_s_app_state_moves_through_starting_to_ready_or_failed() {
    let (mut engine, rx) = surface(How::Start {
        fail: false,
        twice: false,
    });
    pump_until(&mut engine, &rx, "ready");
    // The task starts after the first frame, which sees the app not started yet.
    let states = STATES.with_borrow(Clone::clone);
    assert_eq!(
        states,
        [AppState::NotStarted, AppState::Starting, AppState::Ready]
    );
    drop(engine);

    let (mut engine, rx) = surface(How::Start {
        fail: true,
        twice: false,
    });
    let failed = pump_until(&mut engine, &rx, "failed:");
    assert!(failed.contains("catalog unavailable"), "{failed}");
    let states = STATES.with_borrow(Clone::clone);
    assert_eq!(
        states[..2],
        [AppState::NotStarted, AppState::Starting],
        "{states:?}"
    );
    assert_eq!(states.len(), 3, "{states:?}");
    assert!(
        matches!(states.last(), Some(AppState::Failed(message)) if message.contains("Catalog")),
        "{states:?}"
    );
}

#[test]
fn app_state_is_not_started_before_a_start() {
    #[component]
    fn Idle() -> impl IntoElement {
        text(move || format!("{:?}", app_state()))
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(Root::of::<Idle>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    assert_eq!(pump_until(&mut engine, &rx, "NotStarted"), "NotStarted");
}

#[test]
fn start_async_gives_the_root_provider_or_the_app_error() {
    let (mut engine, rx) = surface(How::StartAsync { fail: false });
    let ready = pump_until(&mut engine, &rx, "ready");
    let given = ASYNC
        .with_borrow(Clone::clone)
        .expect("start_async resolved");
    // The root it gave is the surface's.
    assert_eq!(given.replace("root", "ready"), ready);
    drop(engine);

    let (mut engine, rx) = surface(How::StartAsync { fail: true });
    pump_until(&mut engine, &rx, "failed:");
    let given = ASYNC
        .with_borrow(Clone::clone)
        .expect("start_async resolved");
    assert!(given.starts_with("error the app did not start"), "{given}");
    assert!(given.contains("catalog unavailable"), "{given}");
}

#[test]
fn a_second_start_while_the_first_starts_joins_it() {
    let (mut engine, rx) = surface(How::Start {
        fail: false,
        twice: true,
    });
    let ready = pump_until(&mut engine, &rx, "ready");
    // Two builders, one root: the first's (the second builder was dropped).
    let made = MADE.with_borrow(Clone::clone);
    assert_eq!(made.len(), 2, "{made:?}");
    assert_eq!(ready, format!("ready {}", made[0]));
}

#[test]
fn a_start_on_a_ready_surface_keeps_its_root() {
    let (mut engine, rx) = surface(How::AgainWhenReady);
    let ready = pump_until(&mut engine, &rx, "ready");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while AGAIN.with(std::cell::Cell::get).is_none() && std::time::Instant::now() < deadline {
        engine.pump();
    }
    let made = MADE.with_borrow(Clone::clone);
    assert_eq!(made.len(), 3, "{made:?}");
    assert_eq!(ready, format!("ready {}", made[0]));
    // `start_async()` on the ready surface gave the first root.
    assert_eq!(
        ASYNC.with_borrow(Clone::clone),
        Some(format!("root {}", made[0]))
    );
    // And the surface's root after both is still the first.
    assert_eq!(AGAIN.with(std::cell::Cell::get), Some(made[0]));
    assert_eq!(
        STATES.with_borrow(|states| states.last().cloned()),
        Some(AppState::Ready)
    );
}

#[test]
fn two_surfaces_build_two_providers() {
    let (mut first, first_rx) = surface(How::Start {
        fail: false,
        twice: false,
    });
    let (mut second, second_rx) = surface(How::Start {
        fail: false,
        twice: false,
    });
    let first = pump_until(&mut first, &first_rx, "ready");
    let second = pump_until(&mut second, &second_rx, "ready");
    assert_ne!(first, second, "each surface has its own session");
}

#[test]
fn app_errors_name_the_builder_s_start() {
    for error in [
        AppError::NotStarted,
        AppError::Starting,
        AppError::BuilderStarted,
    ] {
        let message = error.to_string();
        assert!(!message.contains("set_app"), "{message}");
        assert!(!message.contains(concat!("App", "Services")), "{message}");
    }
    assert!(AppError::NotStarted.to_string().contains("start"));
}

#[test]
fn a_rust_root_s_storage_is_nsh_s_store_whatever_the_order() {
    use rayx_app_nsh::NshKeyValueStore;
    use rayx_app_nsh::rayx_runtime::services::storage::{KeyValueStore, StorageAppBuilderExt as _};

    #[component]
    fn StoreRoot() -> impl IntoElement {
        let shown = signal(String::from("starting"));
        spawn(async move {
            // `add_storage()` before NSH's services are added (at the start): NSH's store still.
            let root = ::rayx_app::testing::add_host_fakes(RayXAppBuilder::new())
                .add_storage()
                .start_async()
                .await
                .unwrap();
            let store = root.required_service::<dyn KeyValueStore>().unwrap();
            let nsh = root.required_service::<NshKeyValueStore>().unwrap();
            let same = std::ptr::eq(
                Arc::as_ptr(&store).cast::<()>(),
                Arc::as_ptr(&nsh).cast::<()>(),
            );
            shown.set(format!("store nsh {same}"));
        });
        text(move || shown.get())
    }

    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(Root::of::<StoreRoot>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    assert_eq!(pump_until(&mut engine, &rx, "store"), "store nsh true");
}
