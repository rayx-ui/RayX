//! TS `createScope(configure)`, headless: a scope with registrations of its own. For a token it
//! registers (a TS class, a `ServiceToken`, a contract's class) its registration wins in the
//! scope and its descendants over the root's, also once the root made it, while the root and
//! its other scopes resolve as before; its singletons are one object per declaring scope; the
//! root's scoped and transient services resolved in it see its replacements, and the root's
//! singletons are made with the root; a missing dependency throws what it throws in the root;
//! startup registrations are refused, naming the token; `createScope()` behaves as before.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Arc;

use nsh_core::prelude::*;
use rayx_app_nsh::StateCell;
use rayx_app_nsh::ts::{
    ContractError, ContractRegistryExt as _, Exception, ServiceClass, ServiceProvider,
};

/// Why a greeting failed.
#[derive(Debug)]
pub enum GreeterError {
    /// The call did not reach TS.
    Contract(ContractError),
    /// What TS threw.
    Js(String),
}

impl From<ContractError> for GreeterError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<Exception> for GreeterError {
    fn from(exception: Exception) -> Self {
        Self::Js(exception.message)
    }
}

/// Greets, in Rust or in TS.
#[rayx_app_nsh::contract(ts = "GreeterContract")]
pub trait Greeter {
    /// Greets `name`.
    async fn greet(&self, name: String) -> Result<String, GreeterError>;
    /// Its language now.
    #[state]
    fn language(&self) -> &StateCell<String>;
}

/// The greeter for TSX, over whichever greeter is registered.
#[class]
pub struct GreeterService {
    greeter: Arc<dyn Greeter>,
}

impl ServiceClass for GreeterService {
    type Service = dyn Greeter;

    fn wrap(greeter: Arc<dyn Greeter>) -> Self {
        Self { greeter }
    }
}

#[class]
impl GreeterService {
    /// Its language now, read synchronously.
    #[getter]
    pub fn language(&self) -> String {
        self.greeter.language().get().to_string()
    }

    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): GreeterContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// What Rust's `dyn Greeter` in `services` says to `name`.
    pub(crate) async fn greet(services: Instance<ServiceProvider>, name: String) -> String {
        let greeter = services
            .with(|services| services.services().required_service::<dyn Greeter>())
            .unwrap();
        match greeter.greet(name).await {
            Ok(greeting) => greeting,
            Err(error) => format!("{error:?}"),
        }
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>()
            .contract_class::<dyn Greeter, GreeterService>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, GreeterService, Probe, RayXAppBuilder, ServiceToken } from "test/app";

class Clock { constructor(now) { this.now = now; } }
class Missing {}
class Now {}
class English {
  language = new ContractState("en");
  async greet(name) { return `hello ${name}`; }
}
class German {
  language = new ContractState("de");
  async greet(name) { return `hallo ${name}`; }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    const label = new ServiceToken("label");
    const needs = new ServiceToken("needs");
    let stamps = 0;
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingleton(Clock, () => new Clock(1))
      .addSingleton(label, (s) => ({ text: `root ${s.getRequiredService(Clock).now}` }))
      .addScoped(Missing, () => ({ stamp: ++stamps, now: 0 }))
      .addScoped(needs, (s) => s.getRequiredService(Missing))
      .addTransient(Now, (s) => ({ now: s.getRequiredService(Clock).now }))
      .addSingletonAs(GreeterService, () => new English())
      .start();

    // 1. The root made Clock first; the scope's own wins in it and its descendants only.
    const rootClock = services.getRequiredService(Clock);
    const before = services.createScope();
    let made = 0;
    const scope = services.createScope((scope) => {
      scope.addSingleton(Clock, () => { made++; return new Clock(2); });
    });
    const child = scope.createScope();
    const grandchild = child.createScope();
    const sibling = services.createScope();
    lines.push([scope, child, grandchild, services, before, sibling]
      .map((s) => s.getRequiredService(Clock).now).join(" "));

    // 2. One object per declaring scope, shared by its descendants.
    const own = scope.getRequiredService(Clock);
    const other = services.createScope((scope) => scope.addSingleton(Clock, () => new Clock(2)));
    lines.push(`${own === child.getRequiredService(Clock)} ${own === grandchild.getRequiredService(Clock)} ${own !== other.getRequiredService(Clock)} ${rootClock === services.getRequiredService(Clock)} ${made}`);

    // 3. The root's singleton is made with the root, though a scope resolved it first.
    lines.push(`${scope.getRequiredService(label).text} ${services.getRequiredService(label).text}`);

    // 4. The root's scoped and transient services resolved in a scope see its replacements.
    const fake = services.createScope((scope) => {
      scope.addScoped(Missing, () => ({ stamp: "fake" }));
      scope.addSingleton(Clock, () => new Clock(3));
    });
    lines.push(`${fake.getRequiredService(needs).stamp} ${fake.getRequiredService(Now).now} ${services.getRequiredService(Now).now} ${typeof services.getRequiredService(needs).stamp}`);

    // 5. The nearest scope wins; scoped and transient forms keep their lifetimes.
    const inner = child.createScope((scope) => {
      scope.addSingletonAs(Clock, () => new Clock(4));
      scope.addScopedAs(label, () => ({ text: "inner" }));
      scope.addTransientAs(Missing, () => ({ stamp: "new" }));
    });
    const innerChild = inner.createScope();
    lines.push(`${inner.getRequiredService(Clock).now} ${innerChild.getRequiredService(Clock).now} ${child.getRequiredService(Clock).now}`);
    lines.push(`${inner.getRequiredService(label) === inner.getRequiredService(label)} ${inner.getRequiredService(label) !== innerChild.getRequiredService(label)} ${inner.getRequiredService(Missing) !== inner.getRequiredService(Missing)}`);

    // 6. A contract's implementation under its class: Rust's `dyn Greeter` in the scope.
    const german = services.createScope((scope) => scope.addSingletonAs(GreeterService, () => new German()));
    lines.push(`${await Probe.greet(german, "Ana")} ${await Probe.greet(german.createScope(), "Ben")} ${await Probe.greet(services, "Cy")}`);
    lines.push(`${german.getRequiredService(GreeterService).language} ${services.getRequiredService(GreeterService).language}`);

    // 7. A missing dependency throws what it throws in the root.
    const absent = new ServiceToken("absent");
    const broken = (s) => s.getRequiredService(absent);
    const scoped = services.createScope((scope) => scope.addScoped(label, broken));
    const scopeError = thrown(() => scoped.getRequiredService(label));
    lines.push(scopeError);

    // 8. Startup services belong to the root.
    lines.push(thrown(() => services.createScope((scope) => scope.addStartupSingleton(label, () => ({})))));
    lines.push(thrown(() => services.createScope((scope) => scope.addStartupScopedAs(GreeterService, () => new German()))));

    // 9. No argument: a plain scope.
    const plain = services.createScope();
    lines.push(`${plain.getRequiredService(Clock) === rootClock} ${plain.getRequiredService(needs) !== services.getRequiredService(needs)}`);
    scope.dispose();
    lines.push(thrown(() => scope.createScope(() => {})).includes("disposed") ? "disposed" : "used");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scope_with_registrations_of_its_own_wins_for_its_tokens_alone() {
    assert_eq!(
        headless::run(set(), TSX),
        "done 2 2 2 1 1 1\
         |true true true true 1\
         |root 1 root 1\
         |fake 3 1 number\
         |4 4 2\
         |true true true\
         |hallo Ana hallo Ben hello Cy\
         |de en\
         |service `ServiceToken(label)` could not be resolved: its factory threw: no service is registered for ServiceToken(absent)\
         |ServiceToken(label) is a startup service: startup services belong to the root, so register it with the app's builder, not in a scope\
         |GreeterService is a startup service: startup services belong to the root, so register it with the app's builder, not in a scope\
         |true true\
         |disposed"
    );
}

const AFTER_DISPOSE: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

class Clock { constructor(now) { this.now = now; } }
class Page {}
class Stamp {}
class Cache {}

const shown = signal("waiting");
setTimeout(async () => {
  try {
    let pages = 0;
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingleton(Clock, () => new Clock(1))
      .addScoped(Page, () => ({ id: ++pages }))
      .start();
    const scope = services.createScope((scope) => {
      scope.addSingleton(Clock, () => new Clock(2));
      scope.addSingleton(Stamp, (s) => ({ now: s.getRequiredService(Clock).now }));
      scope.addSingleton(Cache, (s) => ({ page: s.getRequiredService(Page) }));
    });
    const first = scope.createScope((scope) => scope.addSingleton(Clock, () => new Clock(3)));
    const second = scope.createScope();
    scope.dispose();

    // First resolved by descendants after the declaring scope is disposed.
    const stamp = first.getRequiredService(Stamp);
    const firstPage = first.getRequiredService(Page);
    const cache = second.getRequiredService(Cache);
    const secondPage = second.getRequiredService(Page);
    shown.set(`done ${stamp.now} ${stamp === second.getRequiredService(Stamp)} ${cache === first.getRequiredService(Cache)} ${cache.page !== firstPage && cache.page !== secondPage}`);
  } catch (error) {
    shown.set(`done threw ${error}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_scopes_singleton_made_after_the_scope_is_disposed_keeps_the_scopes_dependencies() {
    assert_eq!(headless::run(set(), AFTER_DISPOSE), "done 2 true true true");
}

const SAME_ERROR: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder, ServiceToken } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const absent = new ServiceToken("absent");
  const needs = new ServiceToken("needs");
  const broken = (s) => s.getRequiredService(absent);
  const services = await new RayXAppBuilder().addHostFakes().addScoped(needs, broken).start();
  const inRoot = thrown(() => services.getRequiredService(needs));
  const scope = services.createScope((scope) => scope.addScoped(needs, broken));
  const inScope = thrown(() => scope.getRequiredService(needs));
  shown.set(`done ${inRoot === inScope} ${inScope}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_missing_dependency_throws_the_roots_message() {
    assert_eq!(
        headless::run(set(), SAME_ERROR),
        "done true service `ServiceToken(needs)` could not be resolved: its factory threw: \
         no service is registered for ServiceToken(absent)"
    );
}

#[test]
fn the_declarations_type_create_scope_and_its_registrations() {
    let set = set();
    let ts = headless::declarations(&set);
    for declared in [
        "  createScope(configure?: (scope: ScopeRegistrations) => void): ServiceProvider;",
        "export declare class ScopeRegistrations {",
        "  addSingletonAs<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this;",
        "  addScoped<T>(token: ContractClass<T>, factory: (services: ServiceProvider) => T): this;",
        "  addTransient<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this;",
        "  addStartupSingleton(",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
