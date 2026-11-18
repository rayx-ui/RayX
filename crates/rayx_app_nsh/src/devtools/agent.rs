//! The agent on the engine's thread: one per surface, made when it starts. It owns the link to
//! the hub, answers the hub's requests (the tree's on the engine's thread, through NSH's
//! hooks), keeps the tree's revision and the layer's state the hub sets.
//!
//! The link reads frames on a thread of its own (natively) and hands each request to the
//! engine's thread through the surface's dispatcher ([`crate::binding`]); the answer goes back
//! through the link, which may be called from any thread.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nsh_core::inspect::DevDriver;
use nsh_core::prelude::*;
use rayx_devtools_protocol::{self as proto, Id, Params, Route, RpcError, methods};
use serde::Serialize;

use super::DevToolsOptions;
#[cfg(not(target_arch = "wasm32"))]
use super::act;
use super::link::{Event, Link};
use super::tree;

/// Why the agent starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// The app became ready in a dev session.
    Ready,
    /// The app or a test asked ([`super::start_devtools`]).
    Asked,
}

/// How often a watched tree is looked at again.
const WATCH_EVERY: Duration = Duration::from_millis(100);

/// The agent of a surface.
pub(crate) struct Agent {
    id: u64,
    link: Link,
    /// Increases with every change of the tree the agent saw.
    revision: Cell<u64>,
    /// The fingerprint of the tree at `revision`.
    fingerprint: Cell<Option<u64>>,
    /// Each node's parent in the last tree the agent saw, to name a subtree when an id goes.
    parents: RefCell<HashMap<nsh_core::NodeId, nsh_core::NodeId>>,
    /// The worker that takes the driver's steps, once one was asked for.
    #[cfg(not(target_arch = "wasm32"))]
    worker: RefCell<Option<act::Worker>>,
    /// Whether the hub watches the tree (`tree.watch`).
    watching: Cell<bool>,
    /// The current watch loop's generation: an older loop ends when it next wakes, so a quick
    /// off and on leaves one loop looking.
    watch_round: Cell<u64>,
    /// The layer's pick mode, as the hub set it.
    pub(crate) pick_mode: Signal<bool>,
    /// The node the layer outlines as selected, as the hub set it.
    pub(crate) selection: Signal<Option<u64>>,
}

/// The surface's agent, once started.
#[derive(Default)]
struct Slot(Option<Rc<Agent>>);

thread_local! {
    /// The agents of this engine thread's surfaces, by id: a job from the link names one.
    static AGENTS: RefCell<HashMap<u64, Rc<Agent>>> = RefCell::new(HashMap::new());
}

static NEXT_AGENT: AtomicU64 = AtomicU64::new(1);

/// How many times the agent called NSH's tree hooks (`dev_tree`, `dev_node`, `dev_pick`,
/// `dev_source`, `dev_instances`), in this process: the pay-on-connect proof counts them.
static HOOK_CALLS: AtomicU64 = AtomicU64::new(0);

/// One more call of a tree hook.
pub(crate) fn count_hook() {
    HOOK_CALLS.fetch_add(1, Ordering::Relaxed);
}

/// The calls of NSH's tree hooks so far ([`count_hook`]).
pub(crate) fn hook_calls() -> u64 {
    HOOK_CALLS.load(Ordering::Relaxed)
}

/// This surface's agent, when it runs.
pub(crate) fn current() -> Option<Rc<Agent>> {
    try_global::<Slot>()?.with_untracked(|slot| slot.0.clone())
}

/// This surface's agent, read so that the caller follows its start (the layer).
pub(crate) fn tracked() -> Option<Rc<Agent>> {
    global::<Slot>().with(|slot| slot.0.clone())
}

/// The dev session's driver (observe mode), once a host handed it over: the platform's test
/// agent, which takes act, input, wait and screenshot steps.
static DRIVER: std::sync::Mutex<Option<std::sync::Arc<dyn DevDriver>>> =
    std::sync::Mutex::new(None);

/// Registers the agent's observer of the dev session with NSH (`observe_driver`): each dev
/// session a host starts hands it the session's driver.
pub(crate) fn register() {
    nsh_core::inspect::observe_driver(|driver| {
        if let Ok(mut slot) = DRIVER.lock() {
            *slot = Some(driver);
        }
    });
}

/// The dev session's driver, once handed over.
pub(crate) fn driver() -> Option<std::sync::Arc<dyn DevDriver>> {
    DRIVER.lock().ok().and_then(|slot| slot.clone())
}

/// Whether the app runs in a host's dev session: natively a host handed the session's driver
/// over (a plain engine, such as a test's, has none); on the web every dev build's page is one.
fn dev_session() -> bool {
    cfg!(target_arch = "wasm32") || driver().is_some()
}

/// Starts this surface's agent, once.
pub(crate) fn start(
    options: DevToolsOptions,
    why: Why,
    transport: Option<std::sync::Arc<dyn nsh_core::websocket::WebSocketTransport>>,
) {
    if current().is_some() {
        return;
    }
    let config = Config::read();
    if why == Why::Ready && (!config.enabled || !dev_session()) {
        return;
    }
    let url = options
        .url
        .or(config.url)
        .unwrap_or_else(proto::default_url);
    let credential = options.credential.or(config.credential).unwrap_or_default();
    let id = NEXT_AGENT.fetch_add(1, Ordering::Relaxed);
    let hello = proto::encode_request(
        &Id::from(methods::HELLO),
        methods::HELLO,
        &hello(credential),
    );
    let Some(dispatcher) = crate::binding::dispatcher() else {
        return;
    };
    let link = Link::connect(
        url,
        hello,
        move |event| {
            // On the link's thread: the agent hears it on the engine's.
            let _ = dispatcher.unbounded_send(Box::new(move || match event {
                Event::Frame(frame) => receive(id, &frame),
                Event::Closed => closed(id),
            }));
        },
        transport,
    );
    let agent = Rc::new(Agent {
        id,
        link,
        revision: Cell::new(0),
        fingerprint: Cell::new(None),
        parents: RefCell::new(HashMap::new()),
        #[cfg(not(target_arch = "wasm32"))]
        worker: RefCell::new(None),
        watching: Cell::new(false),
        watch_round: Cell::new(0),
        pick_mode: signal(false),
        selection: signal(None),
    });
    AGENTS.with_borrow_mut(|agents| agents.insert(id, Rc::clone(&agent)));
    global::<Slot>().set(Slot(Some(agent)));
}

/// What the configuration says (`DevTools:Url`, `DevTools:Credential`, `DevTools:Enabled`).
struct Config {
    url: Option<String>,
    credential: Option<String>,
    enabled: bool,
}

/// The configuration's value at `key`, unless empty.
fn setting(key: &str) -> Option<String> {
    crate::app_services()
        .ok()?
        .service::<rayx_app::Configuration>()?
        .get::<String>(key)
        .ok()
        .flatten()
        .filter(|value| !value.is_empty())
}

impl Config {
    fn read() -> Self {
        Self {
            url: setting("DevTools:Url"),
            credential: setting("DevTools:Credential"),
            enabled: setting("DevTools:Enabled").is_none_or(|value| value != "false"),
        }
    }
}

/// The app's `hello`, built once per start: its build id changes with every engine start.
fn hello(credential: String) -> proto::Hello {
    static BUILDS: AtomicU64 = AtomicU64::new(0);
    // NSH's wall clock: `std::time::SystemTime::now` and `std::process::id` panic in a browser.
    let started = nsh_core::time::unix_ms().max(0.0) as u64;
    #[cfg(not(target_arch = "wasm32"))]
    let process = std::process::id();
    #[cfg(target_arch = "wasm32")]
    let process = 0;
    let build = BUILDS.fetch_add(1, Ordering::Relaxed);
    let (app_id, app_name) = identity();
    proto::Hello {
        protocol_version: proto::PROTOCOL_VERSION,
        app_id,
        app_name,
        platform: platform().to_owned(),
        build_id: format!("{started:x}-{process}-{build}"),
        surfaces: Vec::new(),
        root: root(),
        credential,
    }
}

/// The app's id and name: the configuration's `DevTools:AppId` and the application's name
/// (`Application:Name`), each falling back to the other.
fn identity() -> (String, String) {
    let name = setting("Application:Name");
    let id = setting("DevTools:AppId");
    match (id, name) {
        (Some(id), Some(name)) => (id, name),
        (Some(id), None) => (id.clone(), id),
        (None, Some(name)) => (name.clone(), name),
        (None, None) => ("app".to_owned(), "app".to_owned()),
    }
}

/// The directory of the app's `nsh.json` as the dev host knows it: `NSH_DEV_APP_DIR` of a
/// desktop dev session. A web page has none: no web host tells it, and a hub that serves a web
/// session knows its root itself.
fn root() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("NSH_DEV_APP_DIR")
            .ok()
            .filter(|dir| !dir.is_empty())
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// The platform, as `hello` names it.
fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// The connection ended: the hub's subscriptions end with it.
fn closed(agent: u64) {
    if let Some(agent) = AGENTS.with_borrow(|agents| agents.get(&agent).cloned()) {
        agent.watch(false);
    }
}

/// A frame from the hub, on the engine's thread.
fn receive(agent: u64, frame: &str) {
    let Some(agent) = AGENTS.with_borrow(|agents| agents.get(&agent).cloned()) else {
        return;
    };
    match proto::route(frame) {
        Route::Request { id, method, params } => agent.request(id, method, params),
        Route::Notification { .. } | Route::Response { .. } | Route::Ignore => {}
        Route::Reply(reply) => agent.link.send(reply),
    }
}

impl Agent {
    /// Answers request `id` of `method`: the tree's questions in a task on the engine's thread,
    /// the layer's at once.
    fn request(self: &Rc<Self>, id: Id, method: &'static str, params: Params<'_>) {
        match method {
            methods::TREE => self.later(id, params.decode(), tree::tree),
            methods::NODE => self.later(id, params.decode(), tree::node),
            methods::FIND => self.later(id, params.decode(), tree::find),
            methods::PICK => self.later(id, params.decode(), tree::pick),
            methods::TREE_WATCH => self.now(
                id,
                params.decode().map(|watch: proto::WatchParams| {
                    self.watch(watch.on);
                    self.done()
                }),
            ),
            methods::LAYER_PICK_MODE => self.now(
                id,
                params.decode().map(|mode: proto::PickMode| {
                    self.pick_mode.set(mode.on);
                    self.done()
                }),
            ),
            methods::LAYER_SELECT => self.now(
                id,
                params.decode().map(|selection: proto::Selection| {
                    self.selection.set(selection.node);
                    self.done()
                }),
            ),
            #[cfg(not(target_arch = "wasm32"))]
            methods::ACT => self.drive(
                id,
                params
                    .decode()
                    .map(|act: proto::Act| (act::act_step(&act), Some(act.node()))),
                None,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            methods::INPUT => self.drive(
                id,
                params
                    .decode()
                    .map(|input: proto::Input| act::input_step(&input)),
                None,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            methods::WAIT => self.drive(
                id,
                params
                    .decode()
                    .map(|wait: proto::WaitParams| (act::wait_step(&wait), None)),
                None,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            methods::SCREENSHOT => match params.decode::<proto::ScreenshotParams>() {
                Ok(shot) => self.drive(
                    id,
                    Ok((serde_json::json!({ "step": "screenshot" }), shot.node)),
                    Some(shot.node),
                ),
                Err(error) => self.now::<()>(id, Err(error)),
            },
            // On the web there is no driver: the hub takes pictures and drives the page itself.
            #[cfg(target_arch = "wasm32")]
            methods::ACT | methods::INPUT | methods::WAIT | methods::SCREENSHOT => {
                self.now::<()>(id, Err(RpcError::unsupported_on_target(method, platform())))
            }
            other => self.now::<()>(id, Err(RpcError::method_not_found(other))),
        }
    }

    /// Sends the answer of request `id` now.
    fn now<R: Serialize>(&self, id: Id, answer: Result<R, RpcError>) {
        self.link.send(match &answer {
            Ok(result) => proto::encode_response(Some(&id), Ok(result)),
            Err(error) => proto::encode_response::<()>(Some(&id), Err(error)),
        });
    }

    /// Answers request `id` with what `run` makes of its parameters, in a task on the engine's
    /// thread (NSH's hooks answer in the next commit).
    fn later<P, R, F>(
        self: &Rc<Self>,
        id: Id,
        params: Result<P, RpcError>,
        run: impl FnOnce(Rc<Agent>, P) -> F + 'static,
    ) where
        P: 'static,
        R: Serialize,
        F: Future<Output = Result<R, RpcError>> + 'static,
    {
        let params = match params {
            Ok(params) => params,
            Err(error) => return self.now::<()>(id, Err(error)),
        };
        let agent = Rc::clone(self);
        drop(spawn(async move {
            let answer = run(Rc::clone(&agent), params).await;
            agent.now(id, answer);
        }));
    }

    /// Takes a driver's `step` on the worker thread once `node` (when it names one) is still
    /// there, and answers request `id` with the revision after it; `picture` makes the answer the
    /// screenshot, cropped to the node it names.
    #[cfg(not(target_arch = "wasm32"))]
    fn drive(
        self: &Rc<Self>,
        id: Id,
        step: Result<(serde_json::Value, Option<u64>), RpcError>,
        picture: Option<Option<u64>>,
    ) {
        let (step, node) = match step {
            Ok(step) => step,
            Err(error) => return self.now::<()>(id, Err(error)),
        };
        let Some(worker) = self.worker() else {
            return self.now::<()>(
                id,
                Err(RpcError::failed(
                    "no dev session drives this app: its host handed no driver over",
                )),
            );
        };
        let agent = Rc::clone(self);
        drop(spawn(async move {
            let mut crop = None;
            // The surface in points: the picture's pixels scale it.
            let window = untracked(window);
            let viewport = proto::Size {
                width: f32::from(window.width),
                height: f32::from(window.height),
            };
            if node.is_some() {
                let tree = tree::snapshot().await;
                let index = tree::Index::of(&tree);
                if let Some(node) = node {
                    match tree::alive(&agent, &index, node) {
                        Ok(alive) => crop = tree::frame_of(&index, alive),
                        Err(error) => return agent.now::<()>(id, Err(error)),
                    }
                }
            }
            let (send, outcome) = futures::channel::oneshot::channel();
            worker.take(act::Job {
                step,
                done: Box::new(move |outcome| {
                    let _ = send.send(outcome);
                }),
            });
            match outcome.await {
                Ok(Ok(value)) => match picture {
                    Some(_) => agent.now(id, act::image(&value, crop, viewport)),
                    None => {
                        let revision = tree::look(&agent).await;
                        agent.now(id, Ok(proto::Done { revision }));
                    }
                },
                Ok(Err(message)) => agent.now::<()>(id, Err(RpcError::failed(message))),
                Err(_) => agent.now::<()>(id, Err(RpcError::failed("the driver went away"))),
            }
        }));
    }

    /// The worker thread on the dev session's driver, started with the first step.
    #[cfg(not(target_arch = "wasm32"))]
    fn worker(&self) -> Option<act::Worker> {
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            *worker = act::Worker::start(driver()?);
        }
        worker.clone()
    }

    /// Notifies the hub of `method` with `params`.
    pub(crate) fn notify(&self, method: &str, params: &impl Serialize) {
        self.link.send(proto::encode_notification(method, params));
    }

    /// The tree's revision the agent saw last.
    pub(crate) fn revision(&self) -> u64 {
        self.revision.get()
    }

    /// Node `id`'s parent in the last tree the agent saw.
    pub(crate) fn parent_of(&self, id: u64) -> Option<u64> {
        let id = nsh_core::NodeId::try_from(id).ok()?;
        self.parents
            .borrow()
            .get(&id)
            .map(|parent| u64::from(*parent))
    }

    /// The answer of a request that changes nothing but says the revision.
    pub(crate) fn done(&self) -> proto::Done {
        proto::Done {
            revision: self.revision.get(),
        }
    }

    /// The revision after seeing a tree with `fingerprint`: one more when it changed.
    pub(crate) fn saw(&self, fingerprint: u64) -> u64 {
        if self.fingerprint.get() != Some(fingerprint) {
            self.fingerprint.set(Some(fingerprint));
            self.revision.set(self.revision.get() + 1);
        }
        self.revision.get()
    }

    /// Remembers each node's parent in the tree the agent just saw.
    pub(crate) fn remember(
        &self,
        parents: impl Iterator<Item = (nsh_core::NodeId, nsh_core::NodeId)>,
    ) {
        *self.parents.borrow_mut() = parents.collect();
    }

    /// The nearest ancestor of node `id`, in the last tree the agent saw, that `alive` says is
    /// still there.
    pub(crate) fn nearest(&self, id: u64, alive: impl Fn(nsh_core::NodeId) -> bool) -> Option<u64> {
        let parents = self.parents.borrow();
        let mut at = nsh_core::NodeId::try_from(id).ok()?;
        while let Some(&parent) = parents.get(&at) {
            if alive(parent) {
                return Some(u64::from(parent));
            }
            at = parent;
        }
        None
    }

    /// Starts or stops watching the tree: while watched, the agent looks at it every
    /// [`WATCH_EVERY`] and notifies `tree.changed` when it changed.
    fn watch(self: &Rc<Self>, on: bool) {
        let was = self.watching.replace(on);
        if on == was {
            return;
        }
        let round = self.watch_round.get() + 1;
        self.watch_round.set(round);
        if !on {
            return;
        }
        let agent = Rc::clone(self);
        let current = move |agent: &Agent| agent.watching.get() && agent.watch_round.get() == round;
        drop(spawn(async move {
            while current(&agent) {
                let before = agent.revision.get();
                let revision = tree::look(&agent).await;
                if revision != before && current(&agent) {
                    agent.link.send(proto::encode_notification(
                        methods::TREE_CHANGED,
                        &proto::TreeChanged { revision },
                    ));
                }
                timer(WATCH_EVERY).await;
            }
        }));
    }

    /// The agent's id in this process.
    #[allow(dead_code)]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
}
