//! The NSH layer through NSH's engine, headless: a root provider per surface, a scope per
//! screen from its mount to its cleanup (Rust components, and TSX through `ServiceScope`), and
//! view models active while their screen is mounted.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use futures::{
    FutureExt,
    future::{LocalBoxFuture, pending},
};
use nsh_core::{
    ApproxTextEngine, MountBatch, MountItem,
    ui::{Registry, Root, prelude::*},
};
use nsh_engine::{App, Bundle, Engine, EngineConfig, LocalEngine, MountSink};
use rayx_app::{
    ActivatableViewModel, AppService, AppServices, AppState, ConfigurationAppBuilderExt,
    RayXAppBuilder, ServiceAppBuilderExt, ServiceScope, ViewModelActivator, app_state,
    di::{
        AsyncInitializable, StartupInitializable, StartupInitializationContext,
        StartupInitializationResult, StartupInitializationState, StartupServiceType,
    },
    services, set_app, use_screen_scope, use_view_model,
};
use tokio::sync::oneshot;

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The tests share the app (one composition root per process) and its counters: one at a time.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

static BUILDS: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
static PAGES_DROPPED: AtomicUsize = AtomicUsize::new(0);

/// A singleton: one per surface.
#[derive(AppService)]
struct Session {
    id: usize,
}

/// A scoped service: one per screen, dropped with it.
#[derive(AppService)]
struct PageState {
    id: usize,
}

impl Drop for PageState {
    fn drop(&mut self) {
        PAGES_DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}

/// A scoped view model with an activator.
#[derive(AppService)]
struct PageViewModel {
    activator: ViewModelActivator,
    page: Arc<PageState>,
}

impl ActivatableViewModel for PageViewModel {
    fn activator(&self) -> &ViewModelActivator {
        &self.activator
    }
}

/// How the catalog's startup goes: initializes, fails, or hangs past its timeout.
static MODE: AtomicUsize = AtomicUsize::new(OK);
const OK: usize = 0;
const FAIL: usize = 1;
const HANG: usize = 2;
/// The store takes 300 ms to initialize.
const SLOW: usize = 3;
/// The pages' options lose their title, which they are validated for on start.
const INVALID_OPTIONS: usize = 4;

/// The startup services that initialized, in order.
static STARTED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// A startup service whose init waits on NSH's timer.
#[derive(Clone, Default, AppService)]
#[app_service(startup)]
struct Store {
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Store {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialized = self.initialized.clone();
        async move {
            let delay = if MODE.load(Ordering::SeqCst) == SLOW {
                300
            } else {
                5
            };
            timer(Duration::from_millis(delay)).await;
            STARTED.lock().unwrap().push("store");
            initialized.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Store {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}

/// A startup service that initializes after `Store`, within 50 ms.
#[derive(Clone, Default, AppService)]
#[app_service(startup)]
struct Catalog {
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
        let initialized = self.initialized.clone();
        match MODE.load(Ordering::SeqCst) {
            FAIL => async { Err("catalog unavailable".into()) }.boxed_local(),
            HANG => pending().boxed_local(),
            _ => async move {
                STARTED.lock().unwrap().push("catalog");
                initialized.mark_initialized();
                Ok(())
            }
            .boxed_local(),
        }
    }

    fn initialization_timeout(&self) -> Option<Duration> {
        Some(Duration::from_millis(50))
    }
}

impl StartupInitializable for Catalog {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<Store>()]
    }
}

/// Sets the catalog's startup for one test, and back to `OK` after it.
struct Mode;

impl Mode {
    fn set(mode: usize) -> Self {
        MODE.store(mode, Ordering::SeqCst);
        STARTED.lock().unwrap().clear();
        Mode
    }
}

impl Drop for Mode {
    fn drop(&mut self) {
        MODE.store(OK, Ordering::SeqCst);
    }
}

/// Options bound to the app's configuration and validated on start.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct PagesOptions {
    title: String,
}

fn next_id() -> usize {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// The app's composition root, as a GameX `with_*` extension would register it.
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    BUILDS.fetch_add(1, Ordering::SeqCst);
    builder
        .configure_app_configuration(|configuration| {
            configuration.add_value(serde_json::json!({ "Pages": { "Title": "Pages" } }));
        })
        .add_options::<PagesOptions>(|options| {
            options
                .bind("Pages")
                .configure(|options| {
                    if MODE.load(Ordering::SeqCst) == INVALID_OPTIONS {
                        options.title.clear();
                    }
                })
                .validate(
                    |options| !options.title.is_empty(),
                    "the pages need a title",
                )
                .validate_on_start();
        })
        .add_startup_singleton(Store::default())
        .add_startup_singleton(Catalog::default())
        .add_singleton(Session { id: next_id() })
        .add_scoped(|_| PageState { id: next_id() })
        .add_scoped(|provider| PageViewModel {
            activator: ViewModelActivator::default(),
            page: provider
                .required_service::<PageState>()
                .expect("the page state is registered"),
        })
}

fn config(app: App, ui_sets: Vec<Registry>) -> EngineConfig {
    // NSH's defaults: a 390 x 844 pt viewport with 44/96 pt header bars, and no network,
    // assets or storage.
    EngineConfig {
        ui_sets,
        ..EngineConfig::new(app)
    }
}

/// The texts in the batches received so far.
fn texts(rx: &Receiver<MountBatch>) -> Vec<String> {
    let mut texts = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(10)) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                texts.push(text.text.clone());
            }
        }
    }
    texts
}

thread_local! {
    /// What closes the second page: sent by the test, on the engine's (the test's) thread.
    static CLOSE: RefCell<Option<oneshot::Receiver<()>>> = const { RefCell::new(None) };
    /// What the pages saw, for the test to compare.
    static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct Seen {
    route: u32,
    session: usize,
    page: usize,
    view_model: Arc<PageViewModel>,
}

/// A screen: its own scope, a scoped view model active while it is mounted.
#[component]
fn Page(props: PageProps) -> impl IntoElement {
    let route = props.route;
    use_screen_scope().expect("the surface has services");
    let view_model = use_view_model::<PageViewModel>().expect("the view model resolves");
    let session = services()
        .and_then(|s| Ok(s.required_service::<Session>()?))
        .expect("the session resolves");
    SEEN.with_borrow_mut(|seen| {
        seen.push(Seen {
            route,
            session: session.id,
            page: view_model.page.id,
            view_model: view_model.clone(),
        })
    });
    text(format!("page {route}"))
}

#[derive(Clone, PartialEq, Props)]
struct PageProps {
    route: u32,
}

/// A Rust root: starts the app and shows a splash until it is ready, then two pages on a
/// stack; the second closes when the test says so.
#[component]
fn Pages() -> impl IntoElement {
    rayx_app::start();
    let routes = signal(vec![0_u32, 1]);
    let close = CLOSE.with(|c| c.borrow_mut().take());
    spawn(async move {
        if let Some(close) = close
            && close.await.is_ok()
        {
            routes.update(|r| r.retain(|route| *route == 0));
        }
    });
    dynamic(move || match app_state() {
        AppState::Ready => nsh_base::nav_stack()
            .flex_1()
            .child(each(
                move || routes.get(),
                |&route: &u32| route,
                move |route, _| {
                    let route = route.get_untracked();
                    nsh_base::nav_page().child(Page::element(PageProps { route }))
                },
            ))
            .into_element(),
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

/// Pumps `engine` until a text starting with `prefix` shows (or 5 s): the texts shown so far.
fn pump_until(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    prefix: &str,
) -> Vec<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut shown = Vec::new();
    while std::time::Instant::now() < deadline {
        engine.pump();
        shown.extend(texts(rx));
        if shown.iter().any(|text| text.starts_with(prefix)) {
            break;
        }
    }
    shown
}

/// One surface running `Pages`, and the test's handle that closes its second page.
fn start_pages() -> (
    LocalEngine<ApproxTextEngine>,
    Receiver<MountBatch>,
    oneshot::Sender<()>,
) {
    let (close, closed) = oneshot::channel();
    CLOSE.with(|c| *c.borrow_mut() = Some(closed));
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        config(App::Rust(Root::of::<Pages>()), Vec::new()),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    engine.pump();
    (engine, rx, close)
}

#[test]
fn nsh_layer_builds_one_root_provider_per_surface() {
    let _serial = serial();
    set_app(build_app);
    SEEN.with_borrow_mut(Vec::clear);
    let builds = BUILDS.load(Ordering::SeqCst);

    let (mut first, first_rx, _close_first) = start_pages();
    let first_shown = pump_until(&mut first, &first_rx, "page 1");
    let first_seen = SEEN.with_borrow_mut(std::mem::take);
    let (mut second, second_rx, _close_second) = start_pages();
    let second_shown = pump_until(&mut second, &second_rx, "page 1");
    let second_seen = SEEN.with_borrow_mut(std::mem::take);

    assert!(first_shown.contains(&"page 1".to_owned()));
    assert!(second_shown.contains(&"page 1".to_owned()));
    assert_eq!(
        BUILDS.load(Ordering::SeqCst) - builds,
        2,
        "the app is built once per surface"
    );
    // A surface's screens share its singleton; another surface has its own.
    assert_eq!(first_seen.len(), 2);
    assert_eq!(first_seen[0].session, first_seen[1].session);
    assert_eq!(second_seen[0].session, second_seen[1].session);
    assert_ne!(first_seen[0].session, second_seen[0].session);
}

#[test]
fn nsh_layer_gives_a_screen_a_scope_from_mount_to_cleanup_and_activates_its_view_model() {
    let _serial = serial();
    set_app(build_app);
    SEEN.with_borrow_mut(Vec::clear);
    let (mut engine, rx, close) = start_pages();
    pump_until(&mut engine, &rx, "page 1");
    let mut seen = SEEN.with_borrow_mut(std::mem::take);
    seen.sort_by_key(|s| s.route);
    assert_eq!(seen.iter().map(|s| s.route).collect::<Vec<_>>(), [0, 1]);
    // Each screen has its own scoped services, active while it is mounted.
    assert_ne!(seen[0].page, seen[1].page);
    assert!(seen.iter().all(|s| s.view_model.activator().is_active()));
    let _ = texts(&rx);

    let dropped = PAGES_DROPPED.load(Ordering::SeqCst);
    let [first, second] = <[Seen; 2]>::try_from(seen).ok().unwrap();
    close.send(()).unwrap();
    engine.pump();
    engine.pump();

    // The closed screen's cleanup deactivated its view model and dropped its scope: once the
    // test lets go of the view model too, nothing holds its scoped services.
    assert!(
        !second.view_model.activator().is_active(),
        "the closed screen deactivated"
    );
    assert!(
        first.view_model.activator().is_active(),
        "the open screen stays active"
    );
    assert_eq!(PAGES_DROPPED.load(Ordering::SeqCst), dropped);
    drop(second);
    assert_eq!(PAGES_DROPPED.load(Ordering::SeqCst) - dropped, 1);

    drop(engine);
    assert!(
        !first.view_model.activator().is_active(),
        "a closed surface deactivates its screens"
    );
}

#[test]
fn nsh_layer_starts_the_app_once_every_startup_service_initialized() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(OK);
    SEEN.with_borrow_mut(Vec::clear);
    let (mut engine, rx, _close) = start_pages();

    // The first frame: the app is starting, nothing resolved, no screen mounted.
    assert_eq!(texts(&rx), ["starting"]);
    assert!(
        STARTED.lock().unwrap().is_empty(),
        "the store waits on its timer"
    );
    assert!(SEEN.with_borrow(Vec::is_empty));

    let shown = pump_until(&mut engine, &rx, "page 1");
    assert!(shown.contains(&"page 0".to_owned()), "{shown:?}");
    // In dependency order, before any screen resolved a service.
    assert_eq!(*STARTED.lock().unwrap(), ["store", "catalog"]);
    assert_eq!(SEEN.with_borrow(Vec::len), 2);
}

#[test]
fn nsh_layer_fails_startup_when_a_service_fails_to_initialize() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(FAIL);
    SEEN.with_borrow_mut(Vec::clear);
    let (mut engine, rx, _close) = start_pages();

    let shown = pump_until(&mut engine, &rx, "failed:");
    let failed = shown
        .iter()
        .find(|text| text.starts_with("failed:"))
        .unwrap();
    assert!(failed.contains("Catalog"), "{failed}");
    assert!(failed.contains("catalog unavailable"), "{failed}");
    assert!(
        !shown.iter().any(|text| text.starts_with("page")),
        "the app never mounts"
    );
    assert!(SEEN.with_borrow(Vec::is_empty));
}

#[test]
fn nsh_layer_fails_startup_when_a_service_times_out() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(HANG);
    let (mut engine, rx, _close) = start_pages();

    let shown = pump_until(&mut engine, &rx, "failed:");
    let failed = shown
        .iter()
        .find(|text| text.starts_with("failed:"))
        .unwrap();
    // The catalog's own timeout (50 ms), not the 30 s default.
    assert!(
        failed.contains("did not initialize within 50 ms"),
        "{failed}"
    );
    assert_eq!(*STARTED.lock().unwrap(), ["store"]);
}

#[test]
fn nsh_layer_fails_startup_when_options_validated_on_start_fail() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(INVALID_OPTIONS);
    SEEN.with_borrow_mut(Vec::clear);
    let (mut engine, rx, _close) = start_pages();

    let shown = pump_until(&mut engine, &rx, "failed:");
    let failed = shown
        .iter()
        .find(|text| text.starts_with("failed:"))
        .unwrap();
    assert!(failed.contains("PagesOptions"), "{failed}");
    assert!(failed.contains("the pages need a title"), "{failed}");
    assert!(
        STARTED.lock().unwrap().is_empty(),
        "no startup service initializes"
    );
    assert!(SEEN.with_borrow(Vec::is_empty));
}

thread_local! {
    /// What makes `Restarter` switch to its second starter.
    static SWITCH: RefCell<Option<oneshot::Receiver<()>>> = const { RefCell::new(None) };
}

/// A component that starts the app (as a Rust root's child would).
#[component]
fn Starter(props: StarterProps) -> impl IntoElement {
    rayx_app::start();
    if props.twice {
        rayx_app::start();
    }
    let name = props.name;
    text(move || format!("{name} {:?}", app_state()))
}

#[derive(Clone, PartialEq, Props)]
struct StarterProps {
    name: String,
    twice: bool,
}

/// Starts the app from a first starter, then (on the test's signal) replaces it with a second
/// one: the first's startup task is cancelled with its owner.
#[component]
fn Restarter() -> impl IntoElement {
    let second = signal(false);
    let switch = SWITCH.with(|s| s.borrow_mut().take());
    spawn(async move {
        if let Some(switch) = switch
            && switch.await.is_ok()
        {
            second.set(true);
        }
    });
    dynamic(move || {
        if second.get() {
            Starter::element(StarterProps {
                name: "second".to_owned(),
                twice: false,
            })
            .into_element()
        } else {
            Starter::element(StarterProps {
                name: "first".to_owned(),
                twice: true,
            })
            .into_element()
        }
    })
}

#[test]
fn nsh_layer_shares_one_startup_and_carries_it_on_when_its_caller_is_cancelled() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(SLOW);
    let builds = BUILDS.load(Ordering::SeqCst);
    let (switch, switched) = oneshot::channel();
    SWITCH.with(|s| *s.borrow_mut() = Some(switched));
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        config(App::Rust(Root::of::<Restarter>()), Vec::new()),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    engine.pump();
    // Two starts at once: one startup. It waits on the store's timer.
    assert_eq!(
        texts(&rx).last().map(String::as_str),
        Some("first Starting")
    );
    assert!(STARTED.lock().unwrap().is_empty());

    // The first starter goes before startup finished (the store takes 300 ms): its tasks are
    // cancelled.
    switch.send(()).unwrap();
    engine.pump();
    assert!(
        texts(&rx).contains(&"second Starting".to_owned()),
        "the second starter mounted while the app was still starting"
    );
    let shown = pump_until(&mut engine, &rx, "second Ready");
    assert!(shown.contains(&"second Ready".to_owned()), "{shown:?}");
    // The second start carried the same startup on: built and initialized once.
    assert_eq!(BUILDS.load(Ordering::SeqCst) - builds, 1);
    assert_eq!(*STARTED.lock().unwrap(), ["store", "catalog"]);
}

/// The view model `Probe.page` resolved last.
static TSX_VIEW_MODEL: Mutex<Option<Arc<PageViewModel>>> = Mutex::new(None);

/// What TSX resolves through a `ServiceScope`, for the JS test.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The page id of the scope's view model, activating it until the scope is disposed.
    pub(crate) fn page(scope: Instance<ServiceScope>) -> Result<u32, String> {
        scope.with_mut(|scope| {
            let view_model = scope
                .view_model::<PageViewModel>()
                .map_err(|e| e.to_string())?;
            let page = view_model.page.id as u32;
            *TSX_VIEW_MODEL.lock().unwrap() = Some(view_model);
            Ok(page)
        })
    }

    /// Lets go of the view model `page` resolved last.
    pub(crate) fn release() {
        TSX_VIEW_MODEL.lock().unwrap().take();
    }

    /// Whether the view model `page` last resolved is active.
    pub(crate) fn active() -> bool {
        TSX_VIEW_MODEL
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|view_model| view_model.activator().is_active())
    }

    /// Whether the root's singleton is the one the scope sees.
    pub(crate) fn same_session(
        app: Instance<AppServices>,
        scope: Instance<ServiceScope>,
    ) -> Result<bool, String> {
        let root = app
            .with(|app| app.services().required_service::<Session>())
            .map_err(|e| e.to_string())?;
        let seen = scope
            .with(|scope| scope.services().required_service::<Session>())
            .map_err(|e| e.to_string())?;
        Ok(Arc::ptr_eq(&root, &seen))
    }

    /// The startup services that initialized, in order.
    pub(crate) fn started() -> String {
        STARTED.lock().unwrap().join(",")
    }

    /// How many scoped page states were dropped so far.
    pub(crate) fn dropped() -> u32 {
        PAGES_DROPPED.load(Ordering::SeqCst) as u32
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { AppServices, Probe } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const before = Probe.dropped();
  const app = await AppServices.load();
  lines.push(Probe.started());
  const screen = app.scope();
  const inner = screen.child();
  const page = Probe.page(screen);
  lines.push(String(page === Probe.page(screen) && Probe.active()));
  lines.push(String(page !== Probe.page(inner)));
  lines.push(String(Probe.sameSession(app, screen) && Probe.sameSession(app, inner)));
  inner.dispose();
  lines.push(`${Probe.dropped() - before} ${Probe.active()}`);
  Probe.page(screen);
  screen.dispose();
  lines.push(String(Probe.active()));
  Probe.release();
  lines.push(String(Probe.dropped() - before));
  try { Probe.page(screen); lines.push("used"); } catch (e) { lines.push("disposed"); }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// The last text shown once one starts with `prefix` (or after 10 s).
fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
            }
        }
        if last.starts_with(prefix) {
            break;
        }
    }
    last
}

/// Runs `tsx` with the `test/app` set: the last text once one starts with `done`.
fn run_tsx(tsx: &str) -> String {
    let mut set = Registry::ui_set("test/app");
    set.class::<AppServices>()
        .class::<ServiceScope>()
        .module::<Probe>();
    let dir = std::env::temp_dir().join(format!("rayx-app-tsx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, tsx).unwrap();
    let ui_sets = vec![set];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: ui_sets.iter().map(Into::into).collect(),
    })
    .unwrap();
    let described: Vec<_> = ui_sets.iter().map(Into::into).collect();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    // SAFETY: compiled just now by this runtime.
    let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
    let _engine =
        Engine::start(config(app, ui_sets), || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let last = shown(&rx, "done");
    let _ = std::fs::remove_dir_all(&dir);
    last
}

#[test]
fn nsh_layer_gives_tsx_a_scope_handle_disposed_in_its_cleanup() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(OK);
    // Startup ran before `load` resolved; the view model is the scope's own (one per scope, a
    // new one in a child scope) and active until its scope is disposed, the singleton the
    // surface's; a disposed scope's scoped services go once nothing else holds them, and using
    // it throws.
    assert_eq!(
        run_tsx(TSX),
        "done store,catalog|true|true|true|0 false|false|2|disposed"
    );
}

const TSX_FAILING: &str = r#"import { mount, signal, Text } from "nsh";
import { AppServices } from "test/app";

const shown = signal("waiting");
AppServices.load().then(
  () => shown.set("done loaded"),
  (e) => shown.set(`done rejected ${e.message}`),
);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn nsh_layer_rejects_tsx_load_when_startup_fails() {
    let _serial = serial();
    set_app(build_app);
    let _mode = Mode::set(FAIL);
    let shown = run_tsx(TSX_FAILING);
    assert!(shown.starts_with("done rejected "), "{shown}");
    assert!(shown.contains("catalog unavailable"), "{shown}");
}
