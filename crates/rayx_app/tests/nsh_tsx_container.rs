//! Services written in TS in RayX's container, through NSH's engine, headless: each lifetime's
//! identity (`===`), factories resolving Rust and TS services, `As` under a `ServiceToken`, a
//! disposed scope releasing its TS services, the resolution errors Rust gets off the engine
//! thread and outside a call from JS, and the declarations typing each registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::atomic::{AtomicU32, Ordering};

use nsh_core::prelude::*;
use rayx_app::ts::ServiceProvider;

static RELEASED: AtomicU32 = AtomicU32::new(0);

/// Counted when its Rust value goes (its JS object was released).
#[class]
struct Tracker;

#[class]
impl Tracker {
    /// A new tracker.
    pub fn new() -> Self {
        Self
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        RELEASED.fetch_add(1, Ordering::SeqCst);
    }
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// What resolving `token`'s TS service from another thread gives.
    pub(crate) fn wrong_thread(
        services: Instance<ServiceProvider>,
        token: JsHandle,
        sys: System,
    ) -> String {
        let Some(key) = ServiceProvider::key_of(&sys, &token) else {
            return "no key".into();
        };
        let provider = services.with(|services| services.services().clone());
        std::thread::spawn(
            move || match provider.required_thread_bound_service_by_key(&key) {
                Ok(_) => "resolved".to_owned(),
                Err(error) => error.to_string(),
            },
        )
        .join()
        .unwrap()
    }

    /// What resolving `token`'s TS service from Rust in a task (outside a call from JS) gives.
    pub(crate) async fn outside_a_call(
        services: Instance<ServiceProvider>,
        token: JsHandle,
        sys: System,
    ) -> String {
        let Some(key) = ServiceProvider::key_of(&sys, &token) else {
            return "no key".into();
        };
        let provider = services.with(|services| services.services().clone());
        match provider.required_thread_bound_service_by_key(&key) {
            Ok(_) => "resolved".to_owned(),
            Err(error) => error.to_string(),
        }
    }

    /// How many trackers went so far.
    pub(crate) fn released() -> u32 {
        RELEASED.load(Ordering::SeqCst)
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, Probe, RayXAppBuilder, ServiceToken, Tracker } from "test/app";

class Clock { now() { return 42; } }
class Page {}
class Stamp {}
class Lazy {}
class EnglishGreeter { greet() { return "hello"; } }
const Greeter = new ServiceToken("Greeter");
let pages = 0;
let stamps = 0;

// What a scope's services are, keeping none of them.
function scoped(services) {
  const scope = services.createScope();
  const other = services.createScope();
  const page = scope.getRequiredService(Page);
  const results = [
    page === scope.getRequiredService(Page) && page !== other.getRequiredService(Page),
    page.services === scope,
    page.clock === services.getRequiredService(Clock),
    page.i18n instanceof I18nService,
    services.getRequiredService(Clock) === scope.getRequiredService(Clock)
      && scope.getRequiredService(Clock) === other.getRequiredService(Clock),
  ].join(" ");
  other.dispose();
  return [scope, results];
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    const services = await new RayXAppBuilder()
      .addI18n()
      .addSingleton(Clock, () => new Clock())
      .addScoped(Page, (s) => ({
        id: ++pages,
        tracker: new Tracker(),
        clock: s.getRequiredService(Clock),
        i18n: s.getRequiredService(I18nService),
        services: s,
      }))
      .addTransient(Stamp, () => ({ n: ++stamps }))
      .addSingletonAs(Greeter, () => new EnglishGreeter())
      .addSingleton(Lazy, () => ({ lazy: true }))
      .start();

    // A singleton is one object per root, in every scope.
    lines.push(String(services.getRequiredService(Clock) === services.getRequiredService(Clock)));
    lines.push(String(services.getRequiredService(Clock).now()));
    // Scoped: one per scope; the factory gets the resolving provider and resolves TS and Rust;
    // each disposed scope releases its own (`other` in `scoped`, then `scope`).
    const before = Probe.released();
    const [scope, results] = scoped(services);
    lines.push(results);
    lines.push(String(Probe.released() - before));
    scope.dispose();
    lines.push(String(Probe.released() - before));
    // Transient: new per resolve.
    const first = services.getRequiredService(Stamp);
    const second = services.getRequiredService(Stamp);
    lines.push(`${first !== second} ${second.n - first.n}`);
    // `As` under a `ServiceToken`.
    lines.push(services.getRequiredService(Greeter).greet());
    lines.push(String(services.getService(new ServiceToken("Greeter"))));
    // Rust off the engine thread, made or not.
    lines.push(Probe.wrongThread(services, Clock));
    lines.push(Probe.wrongThread(services, Lazy));
    // Rust outside a call from JS: a service not made yet does not resolve.
    lines.push(await Probe.outsideACall(services, Lazy));
    lines.push(await Probe.outsideACall(services, Clock));
    // A factory that throws, and a RayX class as a token.
    lines.push(thrown(() => new RayXAppBuilder().addSingleton(I18nService, () => ({}))));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn ts_services_live_in_the_container_with_rusts_lifetimes() {
    let set = headless::set(|set| {
        set.module::<Probe>().class::<Tracker>();
    });
    assert_eq!(
        headless::run(set, TSX),
        "done true|42\
         |true true true true true|1|2\
         |true 1\
         |hello|null\
         |service `Clock` is thread-bound and resolves on its own thread only\
         |service `Lazy` is thread-bound and resolves on its own thread only\
         |service `Lazy` could not be resolved: it is a TS service that is not made yet: TS \
         services are made in a call from JS (a TS `getService`), and startup and contract \
         services by `start()`\
         |resolved\
         |I18nService is a RayX service's class: register a TS service under a TS class or a \
         ServiceToken"
    );
}

const FAILING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

class Broken {}
const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const services = await new RayXAppBuilder()
    .addSingleton(Broken, () => { throw new Error("no clock"); })
    .start();
  lines.push(thrown(() => services.getRequiredService(Broken)));
  lines.push(thrown(() => services.getService(Broken)));
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_factory_that_throws_fails_the_resolve_with_its_message() {
    assert_eq!(
        headless::run(headless::set(|_| {}), FAILING),
        "done service `Broken` could not be resolved: its factory threw: no clock\
         |service `Broken` could not be resolved: its factory threw: no clock"
    );
}

#[test]
fn the_declarations_type_each_registration_and_the_generic_token() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for method in [
        "addSingleton",
        "addSingletonAs",
        "addScoped",
        "addScopedAs",
        "addTransient",
        "addTransientAs",
    ] {
        let declared = format!(
            "  {method}<T>(token: ServiceKey<T>, factory: (services: ServiceProvider) => T): this;"
        );
        assert!(ts.contains(&declared), "no `{declared}` in\n{ts}");
    }
    for declared in [
        "export type ServiceKey<T> = { readonly prototype: T } | ServiceToken<T>;",
        "export declare class ServiceToken<T> {",
        "  #types: [T];",
        "  getRequiredService<T>(token: ServiceKey<T>): T;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
