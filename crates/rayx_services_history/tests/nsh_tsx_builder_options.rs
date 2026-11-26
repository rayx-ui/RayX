//! `addHistory` and `addPersistentHistory` in TS through NSH's engine, headless: their options
//! callback sets `HistoryOptions` as Rust's does, what it sets applies after the options bound
//! from `appsettings.json` (a value it sets overrides the file's), a callback that throws makes
//! the call throw with its message, and the options class is declared with Rust's methods.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app::AppBuilder as _;
use rayx_app::services::operations::OperationService;
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::{FakeKeyValueStore, FakeOperations};
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::{RayXAppBuilder, ServiceProvider};
use rayx_services_history::HistoryOptions;

struct Fakes;

#[class_extension(RayXAppBuilder)]
impl Fakes {
    pub fn add_fake_operations(
        this: Instance<RayXAppBuilder>,
    ) -> Result<Instance<RayXAppBuilder>, String> {
        RayXAppBuilder::extend_with(this, |mut builder| {
            let services = builder.services_mut();
            services.add_singleton_as::<dyn OperationService>(
                FakeOperations::new() as Arc<dyn OperationService>
            );
            services.add_singleton_as::<dyn KeyValueStore>(
                FakeKeyValueStore::new() as Arc<dyn KeyValueStore>
            );
            builder
        })
    }
}

/// What the test reads of the app's options from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The history options the history was made with.
    pub(crate) fn options(services: Instance<ServiceProvider>) -> Result<JsonValue, String> {
        let options = services
            .with(|services| services.services().options::<HistoryOptions>())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(options.cloned()).map_err(|error| error.to_string())
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_history::register(set);
        set.module::<Probe>().class_extension::<Fakes>();
    })
}

/// The app's files: its settings.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => Ok(br#"{ "RayX": { "Services": {
                "History": { "CallTimeoutMs": 1000 }
            } } }"#
                .to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// An app started with `add` (an expression of `builder`) over the file's settings: the
/// history's `call_timeout_ms` (one app per run: a surface has one root).
fn layered(add: &str) -> String {
    let tsx = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  let line;
  try {
    const builder = new RayXAppBuilder().addHostFakes().addAppSettings("app").addFakeOperations();
    const services = await ADD.start();
    line = `${Probe.options(services).call_timeout_ms}`;
  } catch (error) {
    line = `threw ${error}`;
  }
  shown.set(`done ${line}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#
    .replace("ADD", add);
    testing::run_with_assets(set(), &tsx, Arc::new(AppFiles))
}

#[test]
fn options_set_in_ts_apply_after_those_from_app_settings() {
    // The file's, then the callback's over it, then no limit, for both forms.
    assert_eq!(layered("builder.addHistory()"), "done 1000");
    assert_eq!(
        layered("builder.addHistory((options) => { options.callTimeoutMs(5000); })"),
        "done 5000"
    );
    assert_eq!(
        layered("builder.addPersistentHistory((options) => options.callTimeoutMs(250))"),
        "done 250"
    );
    assert_eq!(
        layered("builder.addPersistentHistory((options) => options.callTimeoutMs(null))"),
        "done null"
    );
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.addHistory(() => { throw new Error("no history"); })));
  lines.push(thrown(() => builder.addPersistentHistory(() => { throw new Error("no store"); })));
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_the_call_throw_with_its_message() {
    assert_eq!(testing::run(set(), THROWING), "done no history|no store");
}

#[test]
fn the_history_options_class_is_declared_with_rust_s_methods() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class HistoryOptions {",
        "  callTimeoutMs(timeout?: number | null): HistoryOptions;",
        "  recordAgentOperations(enabled: boolean): HistoryOptions;",
        "addHistory(configure?: ((options: HistoryOptions) => void) | null): RayXAppBuilder;",
        "addPersistentHistory(configure?: ((options: HistoryOptions) => void) | null): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}

#[test]
fn record_agent_operations_defaults_true_and_can_be_toggled_in_ts() {
    let run = |add: &str| {
        let tsx = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const builder = new RayXAppBuilder().addHostFakes().addFakeOperations();
  const services = await ADD.start();
  shown.set(`done ${Probe.options(services).record_agent_operations}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#
        .replace("ADD", add);
        testing::run(set(), &tsx)
    };
    assert_eq!(run("builder.addHistory()"), "done true");
    assert_eq!(
        run("builder.addHistory((options) => options.recordAgentOperations(false))"),
        "done false"
    );
}
