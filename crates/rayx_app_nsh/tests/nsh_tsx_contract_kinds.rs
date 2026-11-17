//! A contract's five kinds of members, implemented in TS, headless: commands awaited and posted,
//! published state (`ContractState`) and events (`ContractEvents`) that Rust reads and follows
//! from any thread, and constants read once when `start()` makes the implementation; and what
//! the set declares for each.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::{Arc, Mutex};

use nsh_core::prelude::*;
use rayx_app_nsh::ts::{ContractError, ContractRegistryExt as _, Exception, ServiceProvider};
use rayx_app_nsh::{EventStream, StateCell};
use serde::{Deserialize, Serialize};

/// Why loading a theme failed.
#[derive(Debug)]
pub enum ThemeError {
    /// The call did not reach TS.
    Contract(ContractError),
    /// What TS threw.
    Js(String),
}

impl From<ContractError> for ThemeError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<Exception> for ThemeError {
    fn from(exception: Exception) -> Self {
        Self::Js(exception.message)
    }
}

/// A theme's colors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    accent: String,
    dark: bool,
}

/// The app's theme: written in TS.
#[rayx_app_nsh::contract]
pub trait Theme {
    /// Loads the theme `name`.
    async fn load(&self, name: String) -> Result<String, ThemeError>;
    /// Notes `line`.
    fn note(&self, line: String);
    /// The theme's name now.
    #[state]
    fn current(&self) -> &StateCell<String>;
    /// Its colors.
    #[state(ts = "{ accent: string; dark: boolean }")]
    fn palette(&self) -> &StateCell<Palette>;
    /// Each change of the theme, by number.
    #[events]
    fn changes(&self) -> &EventStream<u32>;
    /// The theme's version.
    #[constant]
    fn version(&self) -> u32;
    /// The themes there are.
    #[constant]
    fn names(&self) -> Vec<String>;
}

/// What the subscribers heard, in order.
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn log(line: String) {
    LOG.lock().unwrap().push(line);
}

fn theme(services: &Instance<ServiceProvider>) -> Arc<dyn Theme> {
    services
        .with(|services| services.services().required_service::<dyn Theme>())
        .unwrap()
}

/// `read` of the theme, on another thread.
fn elsewhere<T: Send + 'static>(
    services: &Instance<ServiceProvider>,
    read: impl FnOnce(Arc<dyn Theme>) -> T + Send + 'static,
) -> T {
    let provider = services.with(|services| services.services().clone());
    std::thread::spawn(move || read(provider.required_service::<dyn Theme>().unwrap()))
        .join()
        .unwrap()
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The theme's name, read here and on another thread.
    pub(crate) fn current(services: Instance<ServiceProvider>) -> String {
        let here = theme(&services).current().get().to_string();
        let there = elsewhere(&services, |theme| theme.current().get().to_string());
        format!("{here}/{there}")
    }

    /// The palette Rust reads.
    pub(crate) fn palette(services: Instance<ServiceProvider>) -> String {
        let palette = theme(&services).palette().get();
        format!("{} {}", palette.accent, palette.dark)
    }

    /// Follows the name (here) and the changes (here as `a`, from another thread as `b`).
    pub(crate) fn follow(services: Instance<ServiceProvider>) {
        let theme = theme(&services);
        theme
            .current()
            .subscribe(|name| log(format!("state {name}")))
            .detach();
        theme
            .changes()
            .subscribe(|change| log(format!("a {change}")))
            .detach();
    }

    /// Follows the changes from another thread, as `b`.
    pub(crate) fn follow_elsewhere(services: Instance<ServiceProvider>) {
        elsewhere(&services, |theme| {
            theme
                .changes()
                .subscribe(|change| log(format!("b {change}")))
                .detach();
        });
    }

    /// The theme's name, read here and then on another thread, which this task awaits (the
    /// engine thread never waits for it): a new adapter there links through the engine.
    pub(crate) async fn current_awaiting(services: Instance<ServiceProvider>) -> String {
        let here = theme(&services).current().get().to_string();
        let provider = services.with(|services| services.services().clone());
        let (send, receive) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let theme = provider.required_service::<dyn Theme>().unwrap();
            let _ = send.send(format!("{} {}", theme.current().get(), theme.version()));
        });
        format!("{here}/{}", receive.await.unwrap())
    }

    /// The scope's theme resolved and read on another thread and, at the same time, here: both
    /// get the one adapter, linked once, and neither waits on the other.
    pub(crate) async fn resolved_on_both(services: Instance<ServiceProvider>) -> String {
        let provider = services.with(|services| services.services().clone());
        let (send, receive) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let theme = provider.required_service::<dyn Theme>().unwrap();
            let read = theme.current().get().to_string();
            let _ = send.send((theme, read));
        });
        let here = theme(&services);
        let read_here = here.current().get().to_string();
        let (there, read_there) = receive.await.unwrap();
        format!(
            "{read_here} {read_there} {}",
            std::sync::Arc::ptr_eq(&here, &there)
        )
    }

    /// What reading the version gives, twice: its value, or the panic's message.
    pub(crate) fn version_twice(services: Instance<ServiceProvider>) -> String {
        let theme = theme(&services);
        let read = || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| theme.version()))
                .map(|version| version.to_string())
                .unwrap_or_else(|panic| {
                    panic
                        .downcast_ref::<String>()
                        .cloned()
                        .unwrap_or_else(|| "a panic".into())
                })
        };
        let first = read();
        let second = read();
        format!("{first} || {}", first == second)
    }

    /// The constants, read here and on another thread.
    pub(crate) fn constants(services: Instance<ServiceProvider>) -> String {
        let theme = theme(&services);
        let there = elsewhere(&services, |theme| theme.version());
        format!("{} {} {}", theme.version(), there, theme.names().join(","))
    }

    /// What `load(name)` gives Rust.
    pub(crate) async fn load(services: Instance<ServiceProvider>, name: String) -> String {
        let theme = theme(&services);
        match theme.load(name).await {
            Ok(loaded) => loaded,
            Err(error) => format!("{error:?}"),
        }
    }

    /// Notes `line` through the contract.
    pub(crate) fn note(services: Instance<ServiceProvider>, line: String) {
        theme(&services).note(line);
    }

    /// What the subscribers heard, in order.
    pub(crate) fn logged() -> String {
        LOG.lock().unwrap().join(", ")
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>().contract::<dyn Theme>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, Probe, RayXAppBuilder, themeToken } from "test/app";

class MyTheme {
  current = new ContractState("light");
  palette = new ContractState({ accent: "cyan", dark: false });
  changes = new ContractEvents();
  version = 3;
  names = ["light", "dark"];
  noted = [];
  async load(name) {
    if (name === "none") throw new Error("no such theme");
    return `loaded ${name}`;
  }
  note(line) { this.noted.push(line); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(themeToken(), () => new MyTheme())
      .start();
    const theme = services.getRequiredService(themeToken());
    // State: the initial value, here and on another thread, before any publish.
    lines.push(Probe.current(services));
    Probe.follow(services);
    Probe.followElsewhere(services);
    // A publish reaches Rust at once, on every thread, and each subscriber once.
    theme.current.publish("dark");
    lines.push(Probe.current(services));
    theme.palette.publish({ accent: "sky", dark: true });
    lines.push(Probe.palette(services));
    // A value that does not convert throws and keeps the previous one.
    try { theme.current.publish(42); lines.push("took 42"); }
    catch (error) { lines.push(`threw ${error.message.includes("does not convert")}`); }
    lines.push(`${Probe.current(services)} ${theme.current.value}`);
    // Events reach the subscribers in order; a bad one reaches nobody.
    theme.changes.publish(1);
    theme.changes.publish(2);
    try { theme.changes.publish("x"); lines.push("took x"); }
    catch (error) { lines.push(`threw ${error.message.includes("does not convert")}`); }
    lines.push(Probe.logged());
    // Constants: read once, when `start()` made the implementation.
    theme.version = 9;
    theme.names = [];
    lines.push(Probe.constants(services));
    // Commands: awaited, with results and errors, and posted.
    lines.push(await Probe.load(services, "solar"));
    lines.push(await Probe.load(services, "none"));
    Probe.note(services, "hello");
    lines.push(theme.noted.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn rust_reads_and_follows_what_a_ts_implementation_publishes() {
    assert_eq!(
        headless::run(set(), TSX),
        "done light/light\
         |dark/dark\
         |sky true\
         |threw true\
         |dark/dark dark\
         |threw true\
         |state dark, a 1, b 1, a 2, b 2\
         |3 3 light,dark\
         |loaded solar\
         |Js(\"no such theme\")\
         |hello"
    );
}

const MISSING: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, RayXAppBuilder, themeToken } from "test/app";

class Partial {
  current = new ContractState("light");
  palette = new ContractState({ accent: "cyan", dark: false });
  changes = new ContractEvents();
  names = [];
  async load(name) { return name; }
  note() {}
}

const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  .addSingletonAs(themeToken(), () => new Partial())
  .start()
  .then(
    () => shown.set("done started"),
    (error) => shown.set(`done rejected ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_missing_constant_fails_making_the_implementation() {
    let shown = headless::run(set(), MISSING);
    assert!(shown.starts_with("done rejected"), "{shown}");
    assert!(shown.contains("`Theme.version`"), "{shown}");
}

const BAD_STATE: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, RayXAppBuilder, themeToken } from "test/app";

class Wrong {
  current = new ContractState(7);
  palette = new ContractState({ accent: "cyan", dark: false });
  changes = new ContractEvents();
  version = 1;
  names = [];
  async load(name) { return name; }
  note() {}
}

const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  .addSingletonAs(themeToken(), () => new Wrong())
  .start()
  .then(
    () => shown.set("done started"),
    (error) => shown.set(`done rejected ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn state_that_does_not_convert_fails_making_the_implementation() {
    let shown = headless::run(set(), BAD_STATE);
    assert!(shown.starts_with("done rejected"), "{shown}");
    assert!(shown.contains("`Theme.current`"), "{shown}");
}

#[test]
fn the_set_declares_each_kind_of_member() {
    let set = set();
    let ts = headless::declarations(&set);
    for declared in [
        "/** The app's theme: written in TS. */\nexport interface Theme {",
        "  /** Loads the theme `name`. */\n  load(name: string): Promise<string>;",
        "  /** Notes `line`. */\n  note(line: string): void;",
        "  /** The theme's name now. */\n  readonly current: ContractState<string>;",
        "  readonly palette: ContractState<{ accent: string; dark: boolean }>;",
        "  /** Each change of the theme, by number. */\n  readonly changes: ContractEvents<number>;",
        "  /** The theme's version. */\n  readonly version: number;",
        "  readonly names: string[];",
        "export declare class ContractState<T> {",
        "  constructor(initial: T);",
        "  publish(value: T): void;",
        "  get value(): T;",
        "export declare class ContractEvents<E> {",
        "  publish(event: E): void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

const LIFETIMES: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, Probe, RayXAppBuilder, themeToken } from "test/app";

let made = 0;
class Local {
  constructor() { made += 1; this.current = new ContractState(`local ${made}`); }
  palette = new ContractState({ accent: "sky", dark: true });
  changes = new ContractEvents();
  version = 7;
  names = ["local"];
  async load(name) { return name; }
  note() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const scoped = await new RayXAppBuilder().addHostFakes()
      .@ADD@(themeToken(), () => new Local())
      .start();
    const scope = scoped.createScope();
    // Rust resolves the scope's implementation and links it where it reads it: here (between
    // JS's turns, in this task) and on another thread.
    lines.push(await Probe.currentAwaiting(scope));
    lines.push(String(made));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scoped_or_transient_ts_implementation_publishes_too() {
    // Scoped: one object in the scope, linked once, read here and on another thread.
    assert_eq!(
        headless::run(set(), &LIFETIMES.replace("@ADD@", "addScopedAs")),
        "done local 1/local 1 7|1"
    );
    // Transient: each resolve a new object, linked where it is read.
    assert_eq!(
        headless::run(set(), &LIFETIMES.replace("@ADD@", "addTransientAs")),
        "done local 1/local 2 7|2"
    );
}

const BAD_CONSTANT: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, RayXAppBuilder, themeToken } from "test/app";

class Wrong {
  current = new ContractState("light");
  palette = new ContractState({ accent: "cyan", dark: false });
  changes = new ContractEvents();
  version = "x";
  names = [];
  async load(name) { return name; }
  note() {}
}

const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  .addSingletonAs(themeToken(), () => new Wrong())
  .start()
  .then(
    () => shown.set("done started"),
    (error) => shown.set(`done rejected ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_constant_that_does_not_convert_fails_making_the_implementation() {
    let shown = headless::run(set(), BAD_CONSTANT);
    assert!(shown.starts_with("done rejected"), "{shown}");
    assert!(shown.contains("`Theme.version`"), "{shown}");
    assert!(shown.contains("does not convert"), "{shown}");
}

const SCOPED_BAD: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, Probe, RayXAppBuilder, themeToken } from "test/app";

class Wrong {
  current = new ContractState("light");
  palette = new ContractState({ accent: "cyan", dark: false });
  changes = new ContractEvents();
  version = "x";
  names = [];
  async load(name) { return name; }
  note() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const services = await new RayXAppBuilder().addHostFakes()
    .addScopedAs(themeToken(), () => new Wrong())
    .start();
  shown.set(`done ${Probe.versionTwice(services.createScope())}`);
}, 0);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scoped_implementation_with_a_bad_constant_fails_reading_it_naming_it() {
    let shown = headless::run(set(), SCOPED_BAD);
    assert!(
        shown.starts_with(
            "done `Theme.version`: RayX could not link the contract's TS implementation"
        ),
        "{shown}"
    );
    assert!(shown.contains("does not convert"), "{shown}");
    // The failure is kept: the second read fails the same way, without reading JS again.
    assert!(shown.ends_with("|| true"), "{shown}");
}

const BOTH_THREADS: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractEvents, ContractState, Probe, RayXAppBuilder, themeToken } from "test/app";

class Local {
  current = new ContractState("both");
  palette = new ContractState({ accent: "sky", dark: true });
  changes = new ContractEvents();
  version = 1;
  names = [];
  async load(name) { return name; }
  note() {}
}

const shown = signal("waiting");
setTimeout(async () => {
  const services = await new RayXAppBuilder().addHostFakes()
    .addScopedAs(themeToken(), () => new Local())
    .start();
  for (let i = 0; i < 20; i++) {
    const line = await Probe.resolvedOnBoth(services.createScope());
    if (line !== "both both true") { shown.set(`done ${line}`); return; }
  }
  shown.set("done both both true");
}, 0);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scoped_contract_resolved_on_two_threads_at_once_links_once_without_waiting() {
    assert_eq!(headless::run(set(), BOTH_THREADS), "done both both true");
}
