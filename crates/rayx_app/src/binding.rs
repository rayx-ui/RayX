//! dXn.Reactive view models on NSH: their signals, streams, lists and commands as NSH signals in
//! Rust components ([`use_reactive`], [`use_stream`], [`use_two_way`], [`use_list`],
//! [`use_command`]), and as observable properties of TSX classes ([`Observable`],
//! [`ReactiveCommand`]).
//!
//! A view model notifies on the thread that changed it; NSH signals and TSX functions live on the
//! engine's thread. A Rust binding hands each value to a task on the engine's thread, which
//! writes the NSH signal, so a change from any thread reaches the UI; it starts at the view
//! model's current value, so the first frame shows it, and ends with the component's cleanup. A
//! TSX binding tells its listeners on the engine's thread at once, and a change made elsewhere
//! through the surface's dispatcher, which the app's startup runs.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use dxn_reactive::collections::ObservableList;
use dxn_reactive::{
    Command, CommandResult, ReactiveError, ReadSignal as VmReadSignal, Signal as VmSignal,
    Subscription, ValueStream,
};
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use nsh_core::JsonValue;
use nsh_core::prelude::*;
use serde::Serialize;

/// What runs on the engine's thread for a change made on another thread.
type Job = Box<dyn FnOnce() + Send>;

thread_local! {
    /// The surface's dispatcher on this (the engine's) thread, once the app started.
    static DISPATCHER: RefCell<Option<UnboundedSender<Job>>> = const { RefCell::new(None) };
    /// TSX listeners of the bindings by id: a TSX function is not `Send`.
    static LISTENERS: RefCell<HashMap<u64, Vec<Callback<JsonValue>>>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// Starts the task that runs on the engine's thread what bindings send from other threads; it
/// lives as long as the surface. The app's startup calls it, where the engine runs Rust code.
pub(crate) fn start_dispatcher() {
    if DISPATCHER.with_borrow(Option::is_some) {
        return;
    }
    let (send, mut jobs) = unbounded::<Job>();
    DISPATCHER.with_borrow_mut(|dispatcher| *dispatcher = Some(send));
    Owner::detached().run(move || {
        drop(spawn(async move {
            while let Some(job) = jobs.next().await {
                job();
            }
        }));
    });
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
            dispatcher: DISPATCHER.with_borrow(Clone::clone),
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
