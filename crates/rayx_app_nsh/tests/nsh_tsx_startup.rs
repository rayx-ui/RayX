//! Startup services written in TS, through NSH's engine, headless: the interfaces and classes
//! declared; `initialize` awaited once and skipped when `isInitialized()`, asked just before
//! initializing, is true; `startupServiceType`, `initializationDependsOn` and
//! `initializationTimeout` read once; ordering with Rust's startup services both ways; the
//! timeout in milliseconds and the builder's default; failures naming the TS class; the
//! registration token as the key.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::future::LocalBoxFuture;
use nsh_core::prelude::*;
use rayx_app_nsh::AppBuilder as _;
use rayx_app_nsh::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceKeyArg};

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A Rust startup service keyed by a TS token, logging when it initializes.
struct Logged {
    name: &'static str,
    key: StartupServiceType,
    after: Vec<StartupServiceType>,
}

impl AsyncInitializable for Logged {
    fn is_initialized(&self) -> bool {
        false
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        LOG.lock().unwrap().push(self.name.to_owned());
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for Logged {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(self.key.clone())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        self.after.clone()
    }
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds Rust's startup services: `db`, keyed by the TS token `db`, and `report`, after the
    /// TS service registered under `after`.
    pub(crate) fn add_rust(
        builder: Instance<RayXAppBuilder>,
        db: ServiceKeyArg,
        after: ServiceKeyArg,
        sys: System,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        let db = StartupServiceType::runtime(&db.key(&sys));
        let after = StartupServiceType::runtime(&after.key(&sys));
        builder
            .with_mut(|builder| {
                builder.extend(|mut builder| {
                    let services = builder.services_mut();
                    services.add_startup_service(Arc::new(Logged {
                        name: "db",
                        key: db,
                        after: Vec::new(),
                    }));
                    services.add_startup_service(Arc::new(Logged {
                        name: "report",
                        key: StartupServiceType::runtime(&rayx_app_nsh::di::RuntimeKey::new(
                            "Report",
                        )),
                        after: vec![after],
                    }));
                    builder
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }

    /// Logs `line`.
    pub(crate) fn log(line: String) {
        LOG.lock().unwrap().push(line);
    }

    /// What was logged, in order.
    pub(crate) fn logged() -> Vec<String> {
        LOG.lock().unwrap().clone()
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  Probe, RayXAppBuilder, ServiceToken, StartupInitializationState,
} from "test/app";

const Db = new ServiceToken("Db");
const WarmKey = new ServiceToken("WarmKey");
const reads = { type: 0, deps: 0, timeout: 0 };
let cache;
let roots = [];
const skippedState = new StartupInitializationState();

class Cache {
  constructor() { this.state = new StartupInitializationState(); cache = this; }
  isInitialized() { return this.state.isInitialized(); }
  async initialize(context) {
    roots.push(context.services);
    Probe.log("cache");
    // Initializes `Skipped` too, which is asked only after this.
    skippedState.markInitialized();
    this.state.markInitialized();
  }
  initializationDependsOn() { reads.deps++; return [Db]; }
  initializationTimeout() { reads.timeout++; return 1000; }
}
class Skipped {
  isInitialized() { return skippedState.isInitialized(); }
  initialize() { Probe.log("skipped initialized"); }
  initializationDependsOn() { return [Cache]; }
}
class Warm {
  isInitialized() { return false; }
  initialize() { Probe.log("warm"); }
  startupServiceType() { reads.type++; return WarmKey; }
}
class Late {
  isInitialized() { return false; }
  initialize() { Probe.log("late"); }
  initializationDependsOn() { return [WarmKey]; }
}
class Plain {
  constructor() { this.done = false; }
  isInitialized() { return this.done; }
  async initialize() { this.done = true; Probe.log("plain"); }
}
class Scoped extends Plain {}
class Fresh extends Plain {}
const Warmer = new ServiceToken("Warmer");

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const before = (a, b) => {
    const log = Probe.logged();
    return log.includes(a) && log.includes(b) && log.indexOf(a) < log.indexOf(b);
  };
  try {
    const builder = new RayXAppBuilder().addHostFakes()
      .addStartupSingleton(Cache, () => new Cache())
      .addStartupSingleton(Skipped, () => new Skipped())
      .addStartupSingletonAs(Warmer, () => new Warm())
      .addStartupSingleton(Late, () => new Late())
      .addStartupScoped(Scoped, () => new Scoped())
      .addStartupTransient(Fresh, () => new Fresh());
    const services = await Probe.addRust(builder, Db, Cache).start();

    // Ordering: TS after Rust (`Db`), Rust after TS (`Cache`), TS after a TS key it names.
    lines.push(`${before("db", "cache")} ${before("cache", "report")} ${before("warm", "late")}`);
    // Initialized once; `Skipped`, asked just before, was initialized by `Cache` meanwhile.
    const log = Probe.logged();
    lines.push(`${log.filter((l) => l === "cache").length} ${log.includes("skipped initialized")}`);
    // Read once, when the run made it.
    lines.push(`${reads.type} ${reads.deps} ${reads.timeout}`);
    // The initialized objects are the services, the context's provider the root.
    lines.push(String(services.getRequiredService(Cache) === cache && cache.state.isInitialized()));
    lines.push(String(roots.length === 1 && roots[0] === services));
    lines.push(String(services.getRequiredService(Scoped).done));
    lines.push(`${log.filter((l) => l === "plain").length} ${services.getRequiredService(Fresh).done}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn ts_startup_services_initialize_in_the_one_startup_run() {
    assert_eq!(
        headless::run(set(), TSX),
        "done true true true|1 false|1 1 1|true|true|true|2 false"
    );
}

const FAILS: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

class Thrower {
  isInitialized() { return false; }
  initialize() { throw new Error("no disk"); }
}
class Rejecter {
  isInitialized() { return false; }
  async initialize() { throw new Error("no network"); }
}
// Work that never ends (kept, as pending work is: a promise nothing holds is collected).
const pending = [];
class Hanger {
  isInitialized() { return false; }
  initialize() { return new Promise((resolve) => { pending.push(resolve); }); }
  initializationTimeout() { return 30; }
}
class Idle {
  isInitialized() { return false; }
  initialize() { return new Promise((resolve) => { pending.push(resolve); }); }
}

const shown = signal("waiting");
const builder = new RayXAppBuilder().addHostFakes();
@REGISTER@
builder.start().then(
  () => shown.set("done started"),
  (error) => shown.set(`done rejected ${error.message}`),
);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ts_startup_service_that_throws_or_rejects_fails_startup_naming_its_class() {
    let shown = headless::run(
        set(),
        &FAILS.replace(
            "@REGISTER@",
            "builder.addStartupSingleton(Thrower, () => new Thrower())\
             .addStartupSingleton(Rejecter, () => new Rejecter());",
        ),
    );
    assert_eq!(
        shown,
        "done rejected the app did not start: one or more startup services failed: \
         Thrower: no disk; Rejecter: no network"
    );
}

#[test]
fn a_ts_startup_service_times_out_after_its_milliseconds_or_the_default() {
    let shown = headless::run(
        set(),
        &FAILS.replace(
            "@REGISTER@",
            "builder.defaultInitializationTimeout(60)\
             .addStartupSingleton(Hanger, () => new Hanger())\
             .addStartupSingleton(Idle, () => new Idle());",
        ),
    );
    assert_eq!(
        shown,
        "done rejected the app did not start: one or more startup services failed: \
         Hanger: did not initialize within 30 ms; Idle: did not initialize within 60 ms"
    );
}

#[test]
fn a_dependency_on_no_registered_token_fails_naming_both() {
    let shown = headless::run(
        set(),
        &FAILS.replace(
            "@REGISTER@",
            "class Needy { isInitialized() { return false; } initialize() {} \
             initializationDependsOn() { return [Thrower]; } }\n\
             builder.addStartupSingleton(Needy, () => new Needy());",
        ),
    );
    assert_eq!(
        shown,
        "done rejected the app did not start: runtime startup service `Needy` depends on \
         `Thrower`, but no registered startup service matches that dependency"
    );
}

#[test]
fn the_declarations_declare_the_interfaces_classes_and_registrations() {
    let set = set();
    let ts = headless::declarations(&set);
    for declared in [
        "export interface AsyncInitializable {",
        "  isInitialized(): boolean;",
        "  initialize(context: StartupInitializationContext): Promise<void>;",
        "  initializationTimeout?(): number | undefined;",
        "export interface StartupInitializable {",
        "  startupServiceType?(): ServiceKey<unknown>;",
        "  initializationDependsOn?(): ServiceKey<unknown>[];",
        "export declare class StartupInitializationState {",
        "  constructor();",
        "  isInitialized(): boolean;",
        "  markInitialized(): void;",
        "export declare class StartupInitializationContext {",
        "  get services(): ServiceProvider;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    let startup = ts
        .split("export interface StartupInitializable {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    for member in [
        "isInitialized(): boolean;",
        "initialize(context: StartupInitializationContext): Promise<void>;",
        "initializationTimeout?(): number | undefined;",
    ] {
        assert!(
            startup.contains(member),
            "StartupInitializable has no `{member}`"
        );
    }
    for method in [
        "addStartupSingleton",
        "addStartupSingletonAs",
        "addStartupScoped",
        "addStartupScopedAs",
        "addStartupTransient",
        "addStartupTransientAs",
    ] {
        let declared = format!(
            "  {method}<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this;"
        );
        assert!(ts.contains(&declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
