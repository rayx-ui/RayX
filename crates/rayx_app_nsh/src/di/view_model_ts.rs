//! TS view models: `ViewModelActivator`, `ActivationToken` and `DisposeBag` over
//! `dxn_reactive`'s own (so generations and reference counts are its), `useViewModel`, and
//! `Command`, the command of TS view models.
//!
//! The activator's callbacks and the bag's disposables are JS functions: they are kept in
//! [`ThreadBound`] for `dxn_reactive`'s `Send + Sync` bounds and only called on the engine
//! thread, where TS runs.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use dxn_reactive::{ReactiveError, Subscription};
use nsh_core::element::{Arg, JsHost, JsIn, JsKind, JsOwned, PropError, Ret, TsAlias, TsTypes};
use nsh_core::prelude::*;
use rayx_app::di::ThreadBound;

use crate::di::provider_ts::{Resolved, ServiceKeyArg};
use crate::di::screen_ts::{NO_OWNER, ts_services};

/// `ActivatableViewModel` for TS: what `useViewModel` activates.
pub const ACTIVATABLE_VIEW_MODEL: TsAlias = TsAlias {
    name: "ActivatableViewModel",
    ts: "{ readonly activator: ViewModelActivator }",
    doc: "A view model `useViewModel` activates: an object with a `ViewModelActivator` as its \
          `activator`.",
};

/// A JS failure carried through `dxn_reactive`'s errors.
#[derive(Debug)]
struct Failed(String);

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failed {}

/// An object a `DisposeBag` disposes by calling its `dispose()`.
#[interface]
pub trait Disposable {
    /// Ends what it holds.
    fn dispose(&self) -> Result<(), String>;
}

/// What a `DisposeBag` takes: a function, called once, or a `Disposable`.
pub enum Disposal {
    /// A JS function.
    Function(Callback<()>),
    /// An object with `dispose()`.
    Object(JsDisposable),
}

impl Disposal {
    /// Runs it: now where JS can run, else in JS's next turn. What it throws has no caller to
    /// reach (the bag is disposed by an activation ending), as a throwing `onCleanup` has none.
    fn run(&self) {
        match self {
            Self::Function(function) if function.can_run_now() => {
                let _ = function.call(());
            }
            Self::Function(function) => function.post(()),
            Self::Object(object) => {
                let _ = object.dispose();
            }
        }
    }
}

impl JsValue for Disposal {
    fn ts_type(_types: &mut TsTypes) -> String {
        "(() => void) | Disposable".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.kind() == JsKind::Function {
            return <Callback<()> as Arg>::from_js(value).map(Self::Function);
        }
        <JsDisposable as JsValue>::from_js(value).map(Self::Object)
    }

    fn to_js<'a>(&self, _host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Err(PropError("a disposal is not given back to JS".into()))
    }
}

/// Owns disposables and disposes them together, once each: what an activation callback gets
/// for the work of one activation (`bag.add(() => …)`), disposed when the activation ends. A
/// disposable added after that is disposed at once.
#[class]
pub struct DisposeBag {
    bag: Arc<dxn_reactive::DisposeBag>,
}

impl DisposeBag {
    fn of(bag: Arc<dxn_reactive::DisposeBag>) -> Self {
        Self { bag }
    }
}

#[class]
impl DisposeBag {
    /// An empty bag.
    pub fn new() -> Self {
        Self::of(Arc::new(dxn_reactive::DisposeBag::new()))
    }

    /// Takes `disposable` (a function, or an object with `dispose()`): disposed with the bag,
    /// at once when it is disposed already.
    pub fn add(&self, disposable: Disposal) -> Result<(), String> {
        let kept = ThreadBound::new(disposable, "a TS disposable").map_err(|e| e.to_string())?;
        self.bag.add(Subscription::new(move || {
            if let Ok(disposal) = kept.get() {
                disposal.run();
            }
        }));
        Ok(())
    }

    /// Disposes what it holds, once; later disposables are disposed at once.
    pub fn dispose_all(&self) {
        self.bag.dispose();
    }

    /// Whether it was disposed.
    #[getter]
    pub fn is_disposed(&self) -> bool {
        self.bag.is_disposed()
    }
}

impl Default for DisposeBag {
    fn default() -> Self {
        Self::new()
    }
}

/// Keeps a `ViewModelActivator`'s activation alive: disposing the last token of an activation
/// ends it (its bag is disposed). Disposing it again does nothing.
#[class]
pub struct ActivationToken {
    token: dxn_reactive::ActivationToken,
}

#[class]
impl ActivationToken {
    /// Whether it is held and its activation succeeded.
    #[getter]
    pub fn is_active(&self) -> bool {
        self.token.is_active()
    }
}

/// A TS activation callback, kept on the engine thread.
type Activated = ThreadBound<Callback<Instance<DisposeBag>>>;

/// Runs the TS callback `kept` against a bag for `bag`'s activation.
fn run_activated(kept: &Activated, bag: &dxn_reactive::DisposeBag) -> dxn_reactive::Result<()> {
    let callback = kept.get().map_err(ReactiveError::external)?;
    let own = Arc::new(dxn_reactive::DisposeBag::new());
    let ended = Arc::clone(&own);
    bag.add(Subscription::new(move || ended.dispose()));
    if !callback.can_run_now() {
        return Err(ReactiveError::external(Failed(
            "an activation callback runs only where JS runs: activate from JS or a Rust call \
             from it"
                .into(),
        )));
    }
    callback
        .call(Instance::new(DisposeBag::of(own)))
        .map_err(|error| ReactiveError::external(Failed(error)))
}

/// Scopes a view model's work to the time a view is active, by reference count:
/// `whenActivated(bag => …)` registers work that runs against a fresh `DisposeBag` when the
/// first live token is taken (`activate()`); more tokens only raise the count; disposing the
/// last token disposes that activation's bag; a later first token runs every callback again.
/// A callback registered while active runs at once. `dxn_reactive`'s activator, for TS.
#[class]
pub struct ViewModelActivator {
    activator: dxn_reactive::ViewModelActivator,
}

#[class]
impl ViewModelActivator {
    /// An activator with nothing registered.
    pub fn new() -> Self {
        Self {
            activator: dxn_reactive::ViewModelActivator::default(),
        }
    }

    /// Registers `callback` to run against the bag of every activation (now too, while
    /// active); throws what it throws then, and it is not kept.
    pub fn when_activated(
        this: Instance<Self>,
        #[args(bag)] callback: Callback<Instance<DisposeBag>>,
    ) -> Result<(), String> {
        let kept = ThreadBound::new(callback, "a TS activation callback")
            .map_err(|error| error.to_string())?;
        let activator = this.with(|this| this.activator.clone());
        activator
            .when_activated(move |bag| run_activated(&kept, bag))
            .map_err(|error| error.to_string())
    }

    /// A token keeping the view model active until it is disposed; the first live one runs
    /// every registered callback (throws what one throws, and nothing stays active).
    pub fn activate(this: Instance<Self>) -> Result<Instance<ActivationToken>, String> {
        let activator = this.with(|this| this.activator.clone());
        activate(&activator).map(|token| Instance::new(ActivationToken { token }))
    }

    /// Whether an activation is live.
    #[getter]
    pub fn is_active(&self) -> bool {
        self.activator.is_active()
    }
}

impl Default for ViewModelActivator {
    fn default() -> Self {
        Self::new()
    }
}

/// `activator` activated now.
fn activate(
    activator: &dxn_reactive::ViewModelActivator,
) -> Result<dxn_reactive::ActivationToken, String> {
    activator.activate().map_err(|error| error.to_string())
}

/// The activation `useViewModel` keeps until the component's cleanup.
pub(crate) enum Activation {
    /// A TS view model's.
    Ts(Instance<ActivationToken>),
    /// A Rust view model's.
    Rust(dxn_reactive::ActivationToken),
}

impl Activation {
    /// Ends it (the component cleaned up): its token goes.
    pub(crate) fn end(self) {
        match self {
            Self::Ts(token) => token.dispose(),
            Self::Rust(token) => drop(token),
        }
    }
}

/// Activates the TS view model `object` (its `activator`); an error naming `name` when it has
/// none.
pub(crate) fn activate_object(object: &JsHandle, name: &str) -> Result<Activation, String> {
    let activator = object
        .get::<Option<Instance<ViewModelActivator>>>("activator")
        .ok()
        .flatten()
        .ok_or_else(|| not_a_view_model(name))?;
    ViewModelActivator::activate(activator).map(Activation::Ts)
}

/// Why `useViewModel` refuses the service `name` names.
pub(crate) fn not_a_view_model(name: &str) -> String {
    format!(
        "`{name}` is not a view model: register an object with a `ViewModelActivator` as its \
         `activator`, or a Rust view model's class (`view_model_class`)"
    )
}

/// What a `Command`'s action gets: the argument `execute` was called with, as JS passed it.
pub enum Argument {
    /// None (`undefined`).
    None,
    /// An object, array or function, kept.
    Object(JsHandle),
    /// Any other value.
    Value(nsh_core::JsonValue),
}

impl JsValue for Argument {
    fn ts_type(_types: &mut TsTypes) -> String {
        "unknown".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        match value.kind() {
            JsKind::Undefined => Ok(Self::None),
            JsKind::Object | JsKind::Array | JsKind::Function => {
                <JsHandle as JsValue>::from_js(value).map(Self::Object)
            }
            _ => <nsh_core::JsonValue as JsValue>::from_js(value).map(Self::Value),
        }
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Self::None => Ok(JsOwned::new(host, host.undefined())),
            Self::Object(object) => object.to_js(host),
            Self::Value(value) => value.to_js(host),
        }
    }
}

/// An accessor of a boolean (`() => boolean`, an NSH signal or memo), kept to read and to give
/// back.
pub struct BoolAccessor {
    object: JsHandle,
    read: Callback<(), bool>,
}

impl JsValue for BoolAccessor {
    fn ts_type(_types: &mut TsTypes) -> String {
        "import(\"nsh\").Accessor<boolean>".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Ok(Self {
            object: <JsHandle as JsValue>::from_js(value)?,
            read: <Callback<(), bool> as Arg>::from_js(value)?,
        })
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        self.object.to_js(host)
    }
}

/// A command's `canExecute`: the accessor it was made with, else its own.
#[derive(Clone)]
enum CanExecute {
    Given(Rc<BoolAccessor>),
    Always(JsSignal<bool>),
}

impl CanExecute {
    /// Whether it allows a run now (read untracked by its caller).
    fn now(&self) -> Result<bool, String> {
        match self {
            Self::Given(accessor) => accessor.read.call(()),
            Self::Always(_) => Ok(true),
        }
    }
}

/// `Command.canExecute`'s value: an accessor.
pub struct CanExecuteAccessor(CanExecute);

impl Ret for CanExecuteAccessor {
    fn ts_type(types: &mut TsTypes) -> String {
        <BoolAccessor as JsValue>::ts_type(types)
    }

    fn into_js<'a>(self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self.0 {
            CanExecute::Given(accessor) => accessor.to_js(host),
            CanExecute::Always(signal) => signal.into_js(host),
        }
    }
}

/// The command of TS view models: made from an action (`(arg?) => void`, or a promise of it)
/// and an optional `canExecute` accessor. `canExecute()` and `isExecuting()` read as NSH
/// signals (tracked); `await execute(arg?)` runs the action with `arg` when it can run, and
/// rejects saying the command cannot run while `canExecute()` is false or while it runs, so of
/// two overlapping calls the second rejects and the action runs once. `isExecuting()` is true
/// until the action's promise settles; the action's error is rethrown once it is false again.
#[class]
pub struct Command {
    action: Callback<Argument>,
    can_execute: CanExecute,
    executing: JsSignal<bool>,
    running: Rc<Cell<bool>>,
}

#[class]
impl Command {
    /// A command running `action`, while `canExecute` (an accessor; always, without one)
    /// allows it.
    pub fn new(
        #[args(arg)] action: Callback<Argument>,
        can_execute: Option<BoolAccessor>,
        cx: CallCx,
    ) -> Result<Self, String> {
        let can_execute = match can_execute {
            Some(accessor) => CanExecute::Given(Rc::new(accessor)),
            None => CanExecute::Always(cx.signal(true).map_err(|error| error.0)?),
        };
        Ok(Self {
            action,
            can_execute,
            executing: cx.signal(false).map_err(|error| error.0)?,
            running: Rc::default(),
        })
    }

    /// Whether it may run: the accessor it was made with, else one always true.
    #[getter]
    pub fn can_execute(&self) -> CanExecuteAccessor {
        CanExecuteAccessor(self.can_execute.clone())
    }

    /// Whether its action runs now (an NSH signal, tracked).
    #[getter]
    pub fn is_executing(&self) -> JsSignal<bool> {
        self.executing.clone()
    }

    /// Runs the action with `arg`, once it can run; rejects when it cannot (`canExecute()`
    /// false, or running), and with the action's error.
    pub async fn execute(this: Instance<Self>, arg: Option<Argument>) -> Result<(), String> {
        let (action, allowed, executing, running) = this.with(|command| {
            (
                command.action.clone(),
                command.can_execute.now(),
                command.executing.clone(),
                Rc::clone(&command.running),
            )
        });
        if running.get() || !allowed? {
            return Err(
                "the command cannot run now: it is running, or canExecute() is false".into(),
            );
        }
        running.set(true);
        executing.set(true);
        let result = action.call_async(arg.unwrap_or(Argument::None)).await;
        running.set(false);
        executing.set(false);
        result
    }
}

/// TS's `useViewModel`.
#[module]
pub struct ViewModelHooks;

#[module(flat)]
impl ViewModelHooks {
    /// The view model `token` names, resolved in `useServices()` and activated for as long as
    /// the calling component lives (deactivated at its cleanup; a view model two components use
    /// stays active until the last of them cleans up). A TS view model has a
    /// `ViewModelActivator` as its `activator`; a Rust view model's class is registered as one
    /// (`view_model_class`). Throws naming `token` when nothing is registered under it, or when
    /// what is registered is not a view model; nothing is activated then.
    #[ts("<T>(token: ServiceKey<T>): T")]
    pub fn use_view_model(
        token: ServiceKeyArg,
        cx: CallCx,
        sys: System,
    ) -> Result<Resolved, String> {
        if !cx.has_owner() {
            return Err(NO_OWNER.into());
        }
        let services = cx
            .with_owner(|| ts_services(&sys))
            .map_err(|error| error.to_string())?;
        let (resolved, activation) =
            crate::di::provider_ts::resolve_view_model(&services, &token, &sys)?;
        drop(cx.on_cleanup(move || activation.end()));
        Ok(resolved)
    }
}
