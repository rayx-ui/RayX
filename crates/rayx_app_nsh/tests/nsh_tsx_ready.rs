//! Ready hooks written in TS, through NSH's engine, headless: TS and Rust hooks (one from an
//! `apply`'d extension) run in the order added once every startup service initialized, with
//! the root `ServiceProvider`; `onReadyWhen` runs its hook only when its predicate holds; a
//! hook that throws or rejects fails `start()` with its message.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::cell::RefCell;

use nsh_core::prelude::*;
use rayx_app_nsh::{RayXAppBuilder, ReadyAppBuilderExt as _};

thread_local! {
    /// What ran, on the engine's thread (each test has its own engine).
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Adds a Rust ready hook.
#[rayx_app_nsh::app_extension]
fn add_rust_hook(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.on_ready(|_| async {
        LOG.with_borrow_mut(|log| log.push("rust".to_owned()));
        Ok(())
    })
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Logs `line`.
    pub(crate) fn log(line: String) {
        LOG.with_borrow_mut(|log| log.push(line));
    }

    /// What ran, in order.
    pub(crate) fn logged() -> String {
        LOG.with_borrow(|log| log.join(" "))
    }

    /// Where the app is on the surface.
    pub(crate) async fn state() -> String {
        format!("{:?}", rayx_app_nsh::app_state())
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>().class_extension::<add_rust_hook>();
        headless::greeting::register(set);
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { GreetingService, Probe, RayXAppBuilder } from "test/app";

class Warmup {
  isInitialized() { return false; }
  async initialize() { Probe.log("startup"); }
}

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const contexts = [];
  let asked;
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .onReady(async (context) => { contexts.push(context.services); Probe.log("first"); })
      .addRustHook()
      .addStartupSingleton(Warmup, () => new Warmup())
      .onReady(() => Probe.log("second"))
      .onReadyWhen((s) => s.getService(GreetingService) !== null, () => Probe.log("no greeting"))
      .onReadyWhen((s) => { asked = s; return true; }, async () => Probe.log("when"))
      .start();
    lines.push(Probe.logged());
    lines.push(String(contexts.length === 1 && contexts[0] === services && asked === services));
    lines.push(await Probe.state());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn ts_and_rust_ready_hooks_run_in_the_order_added_once_started() {
    assert_eq!(
        headless::run(set(), TSX),
        "done startup first rust second when|true|Ready"
    );
}

const FAILS: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
new RayXAppBuilder().addHostFakes()
  .onReady(() => Probe.log("before"))
  .onReady(@HOOK@)
  .addRustHook()
  .start()
  .then(
    () => shown.set("done started"),
    async (error) => shown.set(`done ${error.message}|${Probe.logged()}|${await Probe.state()}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ready_hook_that_throws_or_rejects_fails_start_with_its_message() {
    for (hook, message) in [
        ("() => { throw new Error(\"no feed\"); }", "no feed"),
        ("async () => { throw new Error(\"no route\"); }", "no route"),
    ] {
        let shown = headless::run(set(), &FAILS.replace("@HOOK@", hook));
        assert_eq!(
            shown,
            format!(
                "done a ready hook failed: {message}|before\
                 |Failed(\"a ready hook failed: {message}\")"
            )
        );
    }
}

#[test]
fn the_declarations_type_the_hooks_and_the_context() {
    let set = set();
    let ts = headless::declarations(&set);
    for declared in [
        "  onReady(hook: (context: ReadyContext) => void | Promise<void>): this;",
        "  onReadyWhen(predicate: (services: ServiceProvider) => boolean, hook: (context: ReadyContext) => void | Promise<void>): this;",
        "export declare class ReadyContext {",
        "  get services(): ServiceProvider;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
