//! dXn.Reactive view models on NSH: their signals, streams, lists and commands as NSH signals in
//! Rust components ([`use_reactive`], [`use_stream`], [`use_two_way`], [`use_list`],
//! [`use_command`]), and as observable properties of TSX classes ([`Observable`],
//! [`ReactiveCommand`]); and the values services publish ([`StateCell`], [`EventStream`]) as an
//! NSH signal and a listener in Rust components ([`use_state_cell`], [`use_event_stream`]).
//!
//! A view model notifies on the thread that changed it; NSH signals and TSX functions live on the
//! engine's thread. A Rust binding hands each value to a task on the engine's thread, which
//! writes the NSH signal, so a change from any thread reaches the UI; it starts at the view
//! model's current value, so the first frame shows it, and ends with the component's cleanup. A
//! TSX binding tells its listeners on the engine's thread at once, and a change made elsewhere
//! through the surface's dispatcher, which the app's startup runs.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::ThreadId;
use std::time::Duration;

use dxn_reactive::collections::ObservableList;
use dxn_reactive::{
    Command, CommandResult, ReactiveError, ReadSignal as VmReadSignal, Signal as VmSignal,
    Subscription, ValueStream,
};
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use nsh_core::JsonValue;
use nsh_core::element::desktop::{QUIT_TIMEOUT, on_app_quit};
use nsh_core::element::reactive::create_root;
use nsh_core::prelude::*;
use rayx_app::{ChangeSubscription, EventStream, StateCell};
use serde::Serialize;

/// What runs on the engine's thread for a change made on another thread.
type Job = Box<dyn FnOnce() + Send>;

/// The surface's dispatcher: what is sent through it runs on its engine's thread.
struct Dispatcher {
    jobs: UnboundedSender<Job>,
    /// The work its runners take.
    work: Arc<Work>,
}

thread_local! {
    /// The surface's dispatcher on this (the engine's) thread, once the app started.
    static DISPATCHER: RefCell<Option<Dispatcher>> = const { RefCell::new(None) };
    /// TSX listeners of the bindings by id: a TSX function is not `Send`.
    static LISTENERS: RefCell<HashMap<u64, Vec<Callback<JsonValue>>>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// How long the app's quit waits for more work once none is waiting and nothing arrived, ran or
/// ended: time for a thread that heard `Destroying` to queue its work.
const QUIT_SETTLE: Duration = Duration::from_millis(50);

/// The longest the app's quit runs the runner's work: inside NSH's [`QUIT_TIMEOUT`] for the quit
/// listeners, so the engine stops when it ends.
const QUIT_DRAIN_LIMIT: Duration = QUIT_TIMEOUT.saturating_sub(Duration::from_millis(50));

/// How often the app's quit looks at the work.
const QUIT_TICK: Duration = Duration::from_millis(2);

/// The work a surface's runners took, as the app's quit waits for it ([`drain`]).
#[derive(Default)]
struct Work {
    /// Jobs queued and not run yet, and work started and not ended yet.
    waiting: AtomicUsize,
    /// Changes each time work arrives, runs or ends.
    activity: AtomicU64,
}

impl Work {
    /// Work arrived: waiting until [`Waiting`] goes.
    fn arrived(self: &Arc<Self>) -> Waiting {
        self.waiting.fetch_add(1, Ordering::SeqCst);
        self.touch();
        Waiting(Arc::clone(self))
    }

    fn touch(&self) {
        self.activity.fetch_add(1, Ordering::SeqCst);
    }

    /// `work`, waiting until it ends (or is dropped), so the app's quit waits for work already
    /// running (a TS store's write, say) up to [`QUIT_DRAIN_LIMIT`].
    fn track<T, F: std::future::Future<Output = T>>(
        self: &Arc<Self>,
        work: F,
    ) -> impl std::future::Future<Output = T> + use<T, F> {
        let waiting = self.arrived();
        async move {
            let output = work.await;
            drop(waiting);
            output
        }
    }
}

/// Work waiting: ran (or dropped unrun) when dropped.
struct Waiting(Arc<Work>);

impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.touch();
        self.0.waiting.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The app quits (an NSH quit listener the host owns): runs what is queued for the engine and
/// the work it starts, as the engine stops, after the app's quit listeners ran (where RayX's
/// suspension service publishes `Destroying`). It ends once nothing is queued or running and
/// nothing arrived, ran or ended for [`QUIT_SETTLE`], or after [`QUIT_DRAIN_LIMIT`]; while it
/// runs, the engine turns, so the queue's task runs each job and the work started is polled.
async fn drain(work: Arc<Work>) {
    let started = nsh_core::time::now();
    let mut seen = work.activity.load(Ordering::SeqCst);
    let mut quiet_since = started;
    loop {
        timer(QUIT_TICK).await;
        let now = nsh_core::time::now();
        let activity = work.activity.load(Ordering::SeqCst);
        if activity != seen || work.waiting.load(Ordering::SeqCst) > 0 {
            seen = activity;
            quiet_since = now;
        } else if now.saturating_duration_since(quiet_since) >= QUIT_SETTLE {
            return;
        }
        if now.saturating_duration_since(started) >= QUIT_DRAIN_LIMIT {
            return;
        }
    }
}

/// Starts the task that runs on the engine's thread what bindings send from other threads; it
/// lives as long as the surface, and runs what is queued as the app quits ([`drain`]). The
/// app's startup calls it, where the engine runs Rust code.
pub(crate) fn start_dispatcher() {
    if DISPATCHER.with_borrow(Option::is_some) {
        return;
    }
    let (send, mut jobs) = unbounded::<Job>();
    let work = Arc::new(Work::default());
    DISPATCHER.with_borrow_mut(|dispatcher| {
        *dispatcher = Some(Dispatcher {
            jobs: send,
            work: Arc::clone(&work),
        });
    });
    Owner::detached().run(move || {
        drop(spawn(async move {
            while let Some(job) = jobs.next().await {
                job();
            }
        }));
        // A root of its own, never disposed: under no owner NSH drops what `on_app_quit` keeps
        // (its cleanup) at once, and the app's quit would not wait for the work.
        create_root(move || on_app_quit(move || drain(Arc::clone(&work))));
    });
}

/// The surface's dispatcher on this (the engine's) thread, once the app started: what is sent
/// through it runs on this thread.
#[cfg(rayx_devtools)]
pub(crate) fn dispatcher() -> Option<UnboundedSender<Job>> {
    DISPATCHER.with_borrow(|dispatcher| dispatcher.as_ref().map(|it| it.jobs.clone()))
}

/// The surface's engine went: what a call through an [`EngineRunner`] gives once the engine
/// stopped (with the call queued or not).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineGone;

impl std::fmt::Display for EngineGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the surface's engine is gone")
    }
}

impl std::error::Error for EngineGone {}

/// Runs work on the surface's engine thread from any thread: what a Rust implementation of a
/// service calls NSH's engine-thread APIs through (fetching `asset:/` files, `register_font`,
/// NSH's storage, `quit`), and what a contract's adapter calls its TS implementation through.
///
/// One per surface, in its root provider (`services.required_service::<EngineRunner>()`);
/// `Send + Sync + Clone`. On the engine thread [`run`](Self::run) starts the work at once (a
/// task where NSH's runtime is entered, the call's op inside a synchronous call from JS; queued
/// where Rust code JS runs outside a call, a stop function say, can start no task); on any
/// other thread it queues it for the engine, which drains the queue in a task of its own (NSH
/// wakes the engine for it). Nothing ever blocks the engine thread waiting for itself.
///
/// As the app quits, after NSH's quit listeners ran (RayX's suspension service publishes
/// `Destroying` from one) and before the engine stops, the host runs what is queued and the work
/// it starts, and what other threads queue meanwhile (a thread that heard `Destroying`, say),
/// until nothing is queued or still running (work already running is waited for) and nothing
/// arrived, ran or ended for 50 ms, within NSH's quit timeout (`QUIT_TIMEOUT`, 200 ms), 150 ms
/// at most; work still running then is dropped when the engine stops. A `run` queued after that resolves `Err(EngineGone)` (a `post` is dropped unrun), and
/// once the engine stopped both give `Err(EngineGone)`.
#[derive(Clone)]
pub struct EngineRunner {
    jobs: UnboundedSender<Job>,
    engine: ThreadId,
    /// How many jobs went through the queue.
    queued: Arc<AtomicU64>,
    /// The surface's work, which the app's quit waits for.
    work: Arc<Work>,
}

impl std::fmt::Debug for EngineRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineRunner")
            .field("engine", &self.engine)
            .field("running", &!self.jobs.is_closed())
            .finish()
    }
}

impl EngineRunner {
    /// The runner of the surface whose engine thread this is, once its dispatcher started
    /// (the app's start starts it).
    pub(crate) fn current() -> Option<Self> {
        DISPATCHER.with_borrow(|dispatcher| {
            dispatcher.as_ref().map(|dispatcher| Self {
                jobs: dispatcher.jobs.clone(),
                engine: std::thread::current().id(),
                queued: Arc::default(),
                work: Arc::clone(&dispatcher.work),
            })
        })
    }

    /// Whether this thread is the surface's engine thread.
    pub fn is_engine_thread(&self) -> bool {
        std::thread::current().id() == self.engine
    }

    /// Runs `make` on the engine thread and the future it makes as an engine task; resolves
    /// with its result, from any thread, or `Err(EngineGone)` once the engine stopped.
    pub fn run<F, Fut, T>(
        &self,
        make: F,
    ) -> futures::future::BoxFuture<'static, Result<T, EngineGone>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + 'static,
        T: Send + 'static,
    {
        let (send, receive) = futures::channel::oneshot::channel();
        if self.is_engine_thread() && runtime_entered() {
            // Where NSH's runtime is entered (a task, a component) the work is made now and
            // spawned. Never through the queue.
            let work = make();
            if start_task(self.work.track(async move {
                let _ = send.send(work.await);
            }))
            .is_err()
            {
                return Box::pin(futures::future::ready(Err(EngineGone)));
            }
        } else if self.is_engine_thread() {
            // Inside a synchronous call from JS it is started as the call's op (made when the
            // core takes it); where no task can start (Rust code JS runs outside a call: a
            // stop function, a finalizer) it is queued, as from another thread.
            let pending = Arc::new(Mutex::new(Some((make, send))));
            let taken = Arc::clone(&pending);
            let started = start_task(self.work.track(async move {
                let pending = taken.lock().unwrap_or_else(PoisonError::into_inner).take();
                if let Some((make, send)) = pending {
                    let _ = send.send(make().await);
                }
            }));
            if started.is_err() {
                let tracked = Arc::clone(&self.work);
                let job = move || {
                    let pending = pending
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    if let Some((make, send)) = pending {
                        let work = make();
                        let _ = start_task(tracked.track(async move {
                            let _ = send.send(work.await);
                        }));
                    }
                };
                if self.queue(Box::new(job)).is_err() {
                    return Box::pin(futures::future::ready(Err(EngineGone)));
                }
            }
        } else {
            let tracked = Arc::clone(&self.work);
            let job = move || {
                let work = make();
                let _ = start_task(tracked.track(async move {
                    let _ = send.send(work.await);
                }));
            };
            if self.queue(Box::new(job)).is_err() {
                return Box::pin(futures::future::ready(Err(EngineGone)));
            }
        }
        Box::pin(async move { receive.await.map_err(|_| EngineGone) })
    }

    /// Runs `job` on the engine thread: at once there where NSH's runtime is entered, as an op
    /// of the call inside a synchronous call from JS, queued for the engine from another thread
    /// (and from Rust code JS runs outside a call, which can start no task); `Err(EngineGone)`
    /// once the engine stopped.
    pub fn post(&self, job: impl FnOnce() + Send + 'static) -> Result<(), EngineGone> {
        if !self.is_engine_thread() {
            return self.queue(Box::new(job));
        }
        if runtime_entered() {
            job();
            return Ok(());
        }
        // The call's op inside a synchronous call from JS; queued where no task can start
        // (Rust code JS runs outside a call: a stop function, a finalizer).
        let pending = Arc::new(Mutex::new(Some(job)));
        let taken = Arc::clone(&pending);
        let started = start_task(async move {
            if let Some(job) = taken.lock().unwrap_or_else(PoisonError::into_inner).take() {
                job();
            }
        });
        if started.is_ok() {
            return Ok(());
        }
        self.queue(Box::new(move || {
            if let Some(job) = pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            {
                job();
            }
        }))
    }

    /// Queues `job` for the engine, waiting (for the app's quit) until it ran.
    fn queue(&self, job: Job) -> Result<(), EngineGone> {
        let waiting = self.work.arrived();
        self.jobs
            .unbounded_send(Box::new(move || {
                job();
                drop(waiting);
            }))
            .map_err(|_| EngineGone)?;
        self.queued.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// How many jobs this surface's runner queued for the engine (work from other threads):
    /// for tests that check the engine thread's work skipped the queue.
    #[doc(hidden)]
    pub fn queued(&self) -> u64 {
        self.queued.load(Ordering::Relaxed)
    }
}

/// Keeps `subscription` until the current owner (the component) cleans up.
fn keep(subscription: Subscription) {
    let kept = RefCell::new(Some(subscription));
    cleanup(move || drop(kept.take()));
}

/// An NSH signal at `initial` that takes every value of `stream`, on the engine's thread, until
/// the component's cleanup. Call it in a component's body.
pub fn use_stream<T>(stream: &ValueStream<T>, initial: T) -> Result<ReadSignal<T>, ReactiveError>
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let target = signal(initial);
    let (send, mut values) = unbounded::<T>();
    keep(stream.subscribe(move |value| {
        let _ = send.unbounded_send(value.clone());
    })?);
    drop(spawn(async move {
        while let Some(value) = values.next().await {
            target.set(value);
        }
    }));
    Ok(target.read_only())
}

/// `source`, a view model's signal, as an NSH signal: its value now and every later one, until
/// the component's cleanup. Call it in a component's body.
pub fn use_reactive<T>(source: &VmReadSignal<T>) -> Result<ReadSignal<T>, ReactiveError>
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    use_stream(&source.values(), source.get()?)
}

/// `source`, a view model's signal, as an NSH signal both ways: the view model's changes reach
/// it, and what the UI writes into it (an input's text) is written back to the view model.
/// Call it in a component's body.
pub fn use_two_way<T>(source: &VmSignal<T>) -> Result<Signal<T>, ReactiveError>
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let target = signal(source.get()?);
    let (send, mut values) = unbounded::<T>();
    keep(source.values().subscribe(move |value| {
        let _ = send.unbounded_send(value.clone());
    })?);
    drop(spawn(async move {
        while let Some(value) = values.next().await {
            target.set(value);
        }
    }));
    let back = source.clone();
    effect(move || {
        let value = target.get();
        // An equal value changes nothing, so the view model's own change stops here.
        let _ = back.set(value);
    });
    Ok(target)
}

/// `list`'s items as an NSH signal, again after every change, until the component's cleanup.
/// Call it in a component's body.
pub fn use_list<T>(list: &ObservableList<T>) -> Result<ReadSignal<Vec<T>>, ReactiveError>
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let target = signal(list.items()?);
    let (send, mut changed) = unbounded::<()>();
    keep(list.connect().subscribe(move |_| {
        let _ = send.unbounded_send(());
    })?);
    let source = list.clone();
    drop(spawn(async move {
        while changed.next().await.is_some() {
            if let Ok(items) = source.items() {
                target.set(items);
            }
        }
    }));
    Ok(target.read_only())
}

/// `cell`, a service's published state, as an NSH signal: its value now and every value published
/// later, from any thread, set on the engine's thread, until the component's cleanup. Call it in a
/// component's body.
pub fn use_state_cell<T>(cell: &StateCell<T>) -> ReadSignal<Arc<T>>
where
    T: PartialEq + Send + Sync + 'static,
{
    // Subscribed before the value is read, so no publish falls between; each change only wakes
    // the task, which reads the cell's value then, so the signal always settles on the latest.
    let (send, mut changed) = unbounded::<()>();
    keep_change(cell.subscribe(move |_| {
        let _ = send.unbounded_send(());
    }));
    let target = signal(cell.get());
    let source = cell.clone();
    drop(spawn(async move {
        while changed.next().await.is_some() {
            target.set(source.get());
        }
    }));
    target.read_only()
}

/// Calls `listener` on the engine's thread with each event `stream` publishes from now on, from
/// any thread, until the component's cleanup. Call it in a component's body.
pub fn use_event_stream<E>(stream: &EventStream<E>, listener: impl Fn(E) + 'static)
where
    E: Clone + Send + Sync + 'static,
{
    let (send, mut events) = unbounded::<E>();
    keep_change(stream.subscribe(move |event| {
        let _ = send.unbounded_send(event.clone());
    }));
    drop(spawn(async move {
        while let Some(event) = events.next().await {
            listener(event);
        }
    }));
}

/// Keeps `subscription` until the current owner (the component) cleans up.
fn keep_change(subscription: ChangeSubscription) {
    let kept = RefCell::new(Some(subscription));
    cleanup(move || drop(kept.take()));
}

/// A view model's command for a Rust component: whether it can execute and whether it is
/// executing, as NSH signals, and [`UiCommand::execute`].
#[derive(Clone)]
pub struct UiCommand {
    /// Whether the command can execute now (bind a button's `disabled` to its negation).
    pub can_execute: ReadSignal<bool>,
    /// Whether an execution is running.
    pub is_executing: ReadSignal<bool>,
    command: Command,
}

impl UiCommand {
    /// Executes the command (refused while it cannot execute).
    pub fn execute(&self) -> CommandResult {
        self.command.execute()
    }
}

/// `command`'s state as NSH signals, until the component's cleanup. Call it in a component's
/// body.
pub fn use_command(command: &Command) -> Result<UiCommand, ReactiveError> {
    Ok(UiCommand {
        can_execute: use_reactive(&command.can_execute())?,
        is_executing: use_reactive(&command.is_executing())?,
        command: command.clone(),
    })
}

/// Tells the TSX listeners of binding `id` about `value`, on the engine's thread.
fn tell(id: u64, value: JsonValue) {
    LISTENERS.with_borrow(|listeners| {
        for listener in listeners.get(&id).into_iter().flatten() {
            listener.post(value.clone());
        }
    });
}

/// Where a binding's change goes: to its listeners at once on the engine's thread, through the
/// dispatcher from another one.
#[derive(Clone)]
struct Notifier {
    id: u64,
    engine: ThreadId,
    dispatcher: Option<UnboundedSender<Job>>,
}

impl Notifier {
    fn new() -> Self {
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        Self {
            id,
            engine: std::thread::current().id(),
            dispatcher: DISPATCHER
                .with_borrow(|dispatcher| dispatcher.as_ref().map(|it| it.jobs.clone())),
        }
    }

    fn notify(&self, value: JsonValue) {
        let id = self.id;
        if std::thread::current().id() == self.engine {
            tell(id, value);
        } else if let Some(dispatcher) = &self.dispatcher {
            let _ = dispatcher.unbounded_send(Box::new(move || tell(id, value)));
        }
    }
}

/// A view model's value for TSX, as a class's property returns it: `value` now, and
/// `bind(setter)` / `onChange(listener)` for every later one, until the object is disposed.
///
/// ```tsx
/// const count = signal(vm.count.value);
/// vm.count.bind(count.set);
/// ```
#[class]
pub struct Observable {
    current: Arc<Mutex<JsonValue>>,
    notifier: Notifier,
    _subscription: Subscription,
}

#[class]
impl Observable {
    /// The value now.
    #[getter]
    pub fn value(&self) -> JsonValue {
        self.current
            .lock()
            .map(|current| current.clone())
            .unwrap_or(JsonValue::Null)
    }

    /// Calls `setter` with the value now and with every later value.
    pub fn bind(&self, #[args(value)] setter: Callback<JsonValue>) {
        setter.post(self.value());
        self.listen(setter);
    }

    /// Calls `listener` with every later value.
    pub fn on_change(&self, #[args(value)] listener: Callback<JsonValue>) {
        self.listen(listener);
    }
}

impl Observable {
    /// `stream`'s values for TSX, starting at `initial`.
    pub fn of_stream<T>(
        stream: &ValueStream<T>,
        initial: &T,
    ) -> Result<Instance<Self>, ReactiveError>
    where
        T: Serialize + Send + Sync + 'static,
    {
        let current = Arc::new(Mutex::new(to_json(initial)));
        let notifier = Notifier::new();
        let (latest, tell) = (Arc::clone(&current), notifier.clone());
        let subscription = stream.subscribe(move |value| {
            let value = to_json(value);
            if let Ok(mut current) = latest.lock() {
                if *current == value {
                    return;
                }
                *current = value.clone();
            }
            tell.notify(value);
        })?;
        Ok(Instance::new(Self {
            current,
            notifier,
            _subscription: subscription,
        }))
    }

    /// `source`, a view model's signal, for TSX.
    pub fn of<T>(source: &VmReadSignal<T>) -> Result<Instance<Self>, ReactiveError>
    where
        T: Clone + Serialize + Send + Sync + 'static,
    {
        Self::of_stream(&source.values(), &source.get()?)
    }

    fn listen(&self, listener: Callback<JsonValue>) {
        LISTENERS.with_borrow_mut(|listeners| {
            listeners
                .entry(self.notifier.id)
                .or_default()
                .push(listener);
        });
    }
}

impl Drop for Observable {
    fn drop(&mut self) {
        LISTENERS.with_borrow_mut(|listeners| listeners.remove(&self.notifier.id));
    }
}

fn to_json(value: &impl Serialize) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

/// A view model's command for TSX: `canExecute` and `isExecuting` as [`Observable`]s, and
/// `execute()`, which throws when the command refuses or fails.
#[class]
pub struct ReactiveCommand {
    command: Command,
}

#[class]
impl ReactiveCommand {
    /// Executes the command.
    pub fn execute(&self) -> Result<(), String> {
        self.command.execute().map_err(|error| error.to_string())
    }

    /// Whether the command can execute now.
    #[getter]
    pub fn can_execute(&self) -> Result<Instance<Observable>, String> {
        Observable::of(&self.command.can_execute()).map_err(|error| error.to_string())
    }

    /// Whether an execution is running.
    #[getter]
    pub fn is_executing(&self) -> Result<Instance<Observable>, String> {
        Observable::of(&self.command.is_executing()).map_err(|error| error.to_string())
    }
}

impl ReactiveCommand {
    /// `command` for TSX.
    pub fn of(command: &Command) -> Instance<Self> {
        Instance::new(Self {
            command: command.clone(),
        })
    }
}
