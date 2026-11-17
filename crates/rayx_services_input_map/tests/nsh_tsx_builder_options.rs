//! `addInputMap` in TS through NSH's engine, headless: its options callback adds bindings as
//! Rust's `InputMapOptions::bind_key` does, after the options bound from `appsettings.json`, and
//! they reach RayX's input map; a callback that throws makes `addInputMap` throw with its
//! message; the options object is valid during its callback only; the options class is declared
//! with Rust's methods.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app::services::storage::KeyValueStore;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_services_input_map::InputMapOptions;

/// Registers a key-value store in memory, which the input map needs.
#[rayx_app_nsh::app_extension]
fn add_fake_storage(mut builder: rayx_app_nsh::RayXAppBuilder) -> rayx_app_nsh::RayXAppBuilder {
    use rayx_app_nsh::AppBuilder as _;
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(
            rayx_app::testing::FakeKeyValueStore::new() as Arc<dyn KeyValueStore>
        );
    builder
}

/// The app's files: its settings, with a framework binding.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => Ok(br#"{ "RayX": { "Services": { "InputMap": {
                "framework": [
                    { "context": "", "binding": { "kind": "keyboard", "value": "cmd-q" }, "action": "quit" }
                ]
            } } } }"#
                .to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// What the test reads of the app's options from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The input map's options the service was made with.
    pub(crate) fn options(services: Instance<ServiceProvider>) -> Result<JsonValue, String> {
        let options = services
            .with(|services| services.services().options::<InputMapOptions>())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(options.cloned()).map_err(|error| error.to_string())
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_input_map::register(set);
        set.module::<Probe>().class_extension::<add_fake_storage>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { InputMapService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const builder = new RayXAppBuilder().addHostFakes().addFakeStorage().addAppSettings("app");
    const chained = builder.addInputMap((options) => {
      options.bindKey("Editor", "ctrl-s", "save").bindKey("Editor", "ctrl-z", "undo");
    });
    lines.push(String(chained === builder));
    const services = await builder.start();

    // The file's framework bindings, and the callback's application ones.
    const options = Probe.options(services);
    lines.push(options.framework.map((entry) => entry.action).join(","));
    lines.push(options.application.map((entry) => `${entry.context}:${entry.action}`).join(","));
    const inputMap = services.getRequiredService(InputMapService);
    const key = (value) => ({ kind: "keyboard", value });
    lines.push(`${inputMap.resolve([], key("cmd-q")).action} ${inputMap.resolve(["Editor"], key("ctrl-s")).action}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn options_set_in_ts_apply_after_configuration_and_reach_the_input_map() {
    let shown = testing::run_with_assets(set(), TSX, Arc::new(AppFiles));
    assert_eq!(shown, "done true|quit|Editor:save,Editor:undo|quit save");
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes().addFakeStorage();
  lines.push(thrown(() => builder.addInputMap(() => { throw new Error("no keys"); })));
  // An options object is valid during its callback only.
  let kept;
  builder.addInputMap((options) => { kept = options; });
  lines.push(thrown(() => kept.bindKey("Editor", "ctrl-q", "quit")).includes("disposed") ? "disposed" : "kept");
  // The builder still works after a callback threw.
  try {
    await builder.addInputMap().start();
    lines.push("started");
  } catch (error) {
    lines.push(`rejected ${error.message}`);
  }
  lines.push(thrown(() => builder.addInputMap()).includes("started") ? "started already" : "kept");
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_add_input_map_throw_with_its_message() {
    assert_eq!(
        testing::run(set(), THROWING),
        "done no keys|disposed|started|started already"
    );
}

#[test]
fn the_options_class_is_declared_with_rust_s_methods() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class InputMapOptions {",
        "  bindKey(context: string, sequence: string, action: string): InputMapOptions;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
