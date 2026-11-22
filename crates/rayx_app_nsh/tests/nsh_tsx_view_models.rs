//! TS view models, headless: `ViewModelActivator`, `ActivationToken` and `DisposeBag` against a
//! fixed trace of `dxn_reactive`'s activation (a TS activator driven from TSX and a Rust one
//! driven natively give the same), `useViewModel` with a TS and a Rust view model shared by two
//! components and its errors, and `Command` with two overlapping calls.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::{Arc, Mutex};

use dxn_reactive::{DisposeBag, Subscription};
use nsh_core::prelude::*;
use rayx_app_nsh::ts::{
    RayXAppBuilder, ServiceClass, ServiceClassRegistryExt as _, ServiceProvider,
};
use rayx_app_nsh::{ActivatableViewModel, AppService, ServiceAppBuilderExt, ViewModelActivator};

/// The trace of one activator's life, as `dxn_reactive`'s activation tests have it: a callback
/// runs against a fresh bag when the first token is taken, a second token only counts, the last
/// token's disposal disposes the generation's bag (each disposable once), a callback registered
/// while active runs at once, a later first token starts a new generation and runs every
/// callback again, and disposing a token twice does nothing.
const TRACE: &str = "active false|A ran|active true|active true|B ran|A disposed|B disposed\
                     |active false|A ran|B ran|A disposed|B disposed|active false";

/// [`TRACE`] produced by `dxn_reactive`'s activator, driven natively.
fn rust_trace() -> String {
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let push = |log: &Arc<Mutex<Vec<String>>>, line: &str| log.lock().unwrap().push(line.into());
    let activator = ViewModelActivator::default();
    let a = Arc::clone(&log);
    activator
        .when_activated(move |bag: &DisposeBag| {
            push(&a, "A ran");
            let a = Arc::clone(&a);
            bag.add(Subscription::new(move || push(&a, "A disposed")));
            Ok(())
        })
        .unwrap();
    let active = |activator: &ViewModelActivator| format!("active {}", activator.is_active());
    push(&log, &active(&activator));
    let first = activator.activate().unwrap();
    push(&log, &active(&activator));
    let second = activator.activate().unwrap();
    drop(first);
    push(&log, &active(&activator));
    let b = Arc::clone(&log);
    activator
        .when_activated(move |bag: &DisposeBag| {
            push(&b, "B ran");
            let b = Arc::clone(&b);
            bag.add(Subscription::new(move || push(&b, "B disposed")));
            Ok(())
        })
        .unwrap();
    drop(second);
    push(&log, &active(&activator));
    let third = activator.activate().unwrap();
    drop(third);
    push(&log, &active(&activator));
    log.lock().unwrap().join("|")
}

const ACTIVATOR: &str = r#"import { mount, signal, Text } from "nsh";
import { ViewModelActivator } from "test/app";

const shown = signal("waiting");
const log = [];
setTimeout(() => {
  try {
    const activator = new ViewModelActivator();
    activator.whenActivated((bag) => {
      log.push("A ran");
      bag.add(() => log.push("A disposed"));
    });
    log.push(`active ${activator.isActive}`);
    const first = activator.activate();
    log.push(`active ${activator.isActive}`);
    const second = activator.activate();
    first.dispose();
    log.push(`active ${activator.isActive}`);
    activator.whenActivated((bag) => {
      log.push("B ran");
      bag.add({ dispose() { log.push("B disposed"); } });
    });
    second.dispose();
    log.push(`active ${activator.isActive}`);
    second.dispose();
    const third = activator.activate();
    third.dispose();
    log.push(`active ${activator.isActive}`);
  } catch (error) {
    log.push(`threw ${error}`);
  }
  shown.set(`done ${log.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ts_activator_and_a_rust_one_give_dxn_reactives_trace() {
    assert_eq!(rust_trace(), TRACE);
    assert_eq!(
        headless::run(headless::set(|_| {}), ACTIVATOR),
        format!("done {TRACE}")
    );
}

// --- useViewModel -------------------------------------------------------------------------

static RUST_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A scoped Rust view model, logging its activations.
#[derive(AppService)]
pub struct PageViewModel {
    activator: ViewModelActivator,
}

impl ActivatableViewModel for PageViewModel {
    fn activator(&self) -> &ViewModelActivator {
        &self.activator
    }
}

/// Its class, a view model's: `useViewModel(PageViewModelClass)`.
#[class]
pub struct PageViewModelClass {
    _view_model: Arc<PageViewModel>,
}

impl ServiceClass for PageViewModelClass {
    type Service = PageViewModel;

    fn wrap(view_model: Arc<PageViewModel>) -> Self {
        Self {
            _view_model: view_model,
        }
    }
}

#[class]
impl PageViewModelClass {}

/// A transient Rust view model: a new one per resolve.
#[derive(AppService)]
pub struct TransientViewModel {
    activator: ViewModelActivator,
}

impl ActivatableViewModel for TransientViewModel {
    fn activator(&self) -> &ViewModelActivator {
        &self.activator
    }
}

/// Its class: `useViewModel(TransientViewModelClass)`.
#[class]
pub struct TransientViewModelClass {
    view_model: Arc<TransientViewModel>,
}

impl ServiceClass for TransientViewModelClass {
    type Service = TransientViewModel;

    fn wrap(view_model: Arc<TransientViewModel>) -> Self {
        Self { view_model }
    }
}

#[class]
impl TransientViewModelClass {
    /// Whether the view model this object wraps is active.
    #[getter]
    pub fn is_active(&self) -> bool {
        self.view_model.activator().is_active()
    }
}

/// The tests' Rust side.
#[module]
struct Pages;

#[module]
impl Pages {
    /// Adds the Rust view model to `builder`.
    pub(crate) fn add_pages(
        builder: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|builder| {
                    builder
                        .add_transient(|_| TransientViewModel {
                            activator: ViewModelActivator::default(),
                        })
                        .add_scoped(|_| {
                            let activator = ViewModelActivator::default();
                            activator
                                .when_activated(|bag| {
                                    RUST_LOG.lock().unwrap().push("rust on".into());
                                    bag.add(Subscription::new(|| {
                                        RUST_LOG.lock().unwrap().push("rust off".into());
                                    }));
                                    Ok(())
                                })
                                .unwrap();
                            PageViewModel { activator }
                        })
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// What the Rust view model logged.
    pub(crate) fn rust_log() -> String {
        RUST_LOG.lock().unwrap().join(",")
    }

    /// Whether `services` resolves `PageViewModel` to an active one.
    pub(crate) fn rust_active(services: Instance<ServiceProvider>) -> bool {
        services.with(|services| {
            services
                .services()
                .required_service::<PageViewModel>()
                .unwrap()
                .activator()
                .is_active()
        })
    }
}

const USE_VIEW_MODEL: &str = r#"import { mount, signal, Text, View } from "nsh";
import {
  GreetingService, PageViewModelClass, Pages, RayXAppBuilder, TransientViewModelClass,
  ViewModelActivator, useScreenScope, useViewModel,
} from "test/app";

const shown = signal("waiting");
const lines = [];
const log = [];
const ready = signal(false);
const a = signal(true);
const b = signal(true);
let screen;
const transients = [];
const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };

class CounterViewModel {
  constructor() {
    this.count = signal(0);
    this.activator = new ViewModelActivator();
    this.activator.whenActivated((bag) => {
      log.push("ts on");
      bag.add(() => log.push("ts off"));
    });
  }
}
class Plain {}
class Missing {}

function Uses(props) {
  const ts = useViewModel(CounterViewModel);
  const rust = useViewModel(PageViewModelClass);
  const transient = useViewModel(TransientViewModelClass);
  transients.push(transient);
  lines.push(`${props.name} ${ts instanceof CounterViewModel} ${rust instanceof PageViewModelClass} ${ts.activator.isActive} ${transient.isActive}`);
  return <Text>{`${props.name}: ${ts.count()}`}</Text>;
}

function Refused() {
  lines.push(thrown(() => useViewModel(Missing)));
  lines.push(thrown(() => useViewModel(GreetingService)));
  lines.push(thrown(() => useViewModel(Plain)));
  return <Text>refused</Text>;
}

function Screen() {
  screen = useScreenScope();
  return (
    <View>
      <If when={a()}><Uses name="a" /></If>
      <If when={b()}><Uses name="b" /></If>
      <Refused />
    </View>
  );
}

setTimeout(async () => {
  try {
    await Pages.addPages(new RayXAppBuilder().addHostFakes().addGreeting())
      .addScoped(CounterViewModel, () => new CounterViewModel())
      .addScoped(Plain, () => new Plain())
      .start();
    ready.set(true);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  setTimeout(() => {
    const ts = screen.getRequiredService(CounterViewModel);
    lines.push(`both ${log.join(",")} ${Pages.rustLog()}`);
    a.set(false);
    setTimeout(() => {
      lines.push(`one left ${ts.activator.isActive} ${Pages.rustActive(screen)}`);
      b.set(false);
      setTimeout(() => {
        lines.push(`none ${ts.activator.isActive} ${Pages.rustActive(screen)} ${log.join(",")} ${Pages.rustLog()}`);
        lines.push(`transients ${transients.length} ${transients[0] !== transients[1]} ${transients.map((t) => t.isActive).join(",")}`);
        shown.set(`done ${lines.join("|")}`);
      }, 20);
    }, 20);
  }, 20);
}, 0);

mount(() => (
  <View>
    <If when={ready()}><Screen /></If>
    <Text>{shown()}</Text>
  </View>
));
"#;

#[test]
fn use_view_model_activates_ts_and_rust_view_models_while_a_component_uses_them() {
    let set = headless::set(|set| {
        set.module::<Pages>()
            .view_model_class::<PageViewModelClass>()
            .view_model_class::<TransientViewModelClass>();
        headless::greeting::register(set);
    });
    let not_a_view_model = |name: &str| {
        format!(
            "`{name}` is not a view model: register an object with a `ViewModelActivator` as \
             its `activator`, or a Rust view model's class (`view_model_class`)"
        )
    };
    assert_eq!(
        headless::run(set, USE_VIEW_MODEL),
        format!(
            "done a true true true true|b true true true true\
             |no service is registered for Missing|{}|{}\
             |both ts on rust on\
             |one left true true\
             |none false false ts on,ts off rust on,rust off\
             |transients 2 true false,false",
            not_a_view_model("GreetingService"),
            not_a_view_model("Plain")
        )
    );
}

// --- Command ------------------------------------------------------------------------------

static STEPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A Rust view model's command, for TSX as a `ReactiveCommand`.
#[class]
pub struct Stepper {
    step: dxn_reactive::Command,
}

#[class]
impl Stepper {
    /// A stepper counting its steps.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            step: dxn_reactive::Command::new(|| {
                STEPS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }),
        }
    }

    /// Its command.
    #[getter]
    pub fn step(&self) -> Instance<rayx_app_nsh::ReactiveCommand> {
        rayx_app_nsh::ReactiveCommand::of(&self.step)
    }

    /// How many steps it took.
    pub fn steps() -> u32 {
        STEPS.load(std::sync::atomic::Ordering::SeqCst) as u32
    }
}

const COMMAND: &str = r#"import { effect, mount, signal, Text, View } from "nsh";
import { Command, Stepper } from "test/app";

const shown = signal("waiting");
const lines = [];
const ran = [];
const busy = [];
const settle = (promise) => promise.then(() => "ok", (error) => error.message);

const command = new Command(async (arg) => {
  ran.push(String(arg));
  await new Promise((resolve) => setTimeout(resolve, 20));
  if (arg === "boom") throw new Error("the action broke");
});
effect(() => busy.push(command.isExecuting()));

function SaveButton(props) {
  const enabled = () => props.command.canExecute() && !props.command.isExecuting();
  const save = () => props.command.execute().catch(() => {});
  presses.save = save;
  const label = () => (enabled() ? "save" : "saving");
  effect(() => labels.push(label()));
  return <View onPress={save}><Text>{label()}</Text></View>;
}

function StepButton(props) {
  const enabled = signal(props.command.canExecute.value);
  props.command.canExecute.bind(enabled.set);
  const step = () => props.command.execute();
  presses.step = step;
  return <View onPress={step}><Text>{enabled() ? "step" : "wait"}</Text></View>;
}

const presses = {};
const labels = [];
const saver = new Command(() => new Promise((resolve) => setTimeout(resolve, 20)));
const stepper = new Stepper();

const allowed = signal(false);
const guarded = new Command(() => ran.push("guarded"), allowed);
const plain = new Command(() => ran.push("plain"));

setTimeout(async () => {
  const first = settle(command.execute("one"));
  const second = settle(command.execute("two"));
  lines.push(`second ${await second}`);
  lines.push(`first ${await first} ${command.isExecuting()}`);
  const failed = await settle(command.execute("boom"));
  lines.push(`failed ${failed.includes("the action broke")} ${command.isExecuting()}`);
  lines.push(`guarded ${guarded.canExecute()} ${await settle(guarded.execute())}`);
  allowed.set(true);
  lines.push(`allowed ${guarded.canExecute()} ${await settle(guarded.execute())}`);
  lines.push(`plain ${plain.canExecute()} ${await settle(plain.execute())}`);
  presses.save();
  await new Promise((resolve) => setTimeout(resolve, 50));
  lines.push(`save button ${labels.join(",")}`);
  presses.step();
  presses.step();
  lines.push(`step button ${Stepper.steps()}`);
  shown.set(`done ${lines.join("|")}|ran ${ran.join(",")}|busy ${busy.join(",")}`);
}, 0);

mount(() => (
  <View>
    <SaveButton command={saver} />
    <StepButton command={stepper.step} />
    <Text>{shown()}</Text>
  </View>
));
"#;

#[test]
fn a_command_ignores_a_call_while_it_runs_and_rethrows_its_actions_error() {
    assert_eq!(
        headless::run(
            headless::set(|set| {
                set.class::<Stepper>();
            }),
            COMMAND
        ),
        "done second the command cannot run now: it is running, or canExecute() is false\
         |first ok false\
         |failed true false\
         |guarded false the command cannot run now: it is running, or canExecute() is false\
         |allowed true ok\
         |plain true ok\
         |save button save,saving,save\
         |step button 2\
         |ran one,boom,guarded,plain\
         |busy false,true,false,true,false"
    );
}

#[test]
fn the_declarations_type_the_view_model_api() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for declared in [
        "export type ActivatableViewModel = { readonly activator: ViewModelActivator };",
        "export declare class ViewModelActivator {",
        "  whenActivated(callback: (bag: DisposeBag) => void): void;",
        "  activate(): ActivationToken;",
        "export declare class ActivationToken {",
        "export declare class DisposeBag {",
        "export interface Disposable {",
        "export declare class Command {",
        "  get isExecuting(): import(\"nsh\").Accessor<boolean>;",
        "export declare function useViewModel<T>(token: ServiceKey<T>): T;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
