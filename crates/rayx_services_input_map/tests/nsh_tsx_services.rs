//! The input map for TSX through NSH's engine, headless: `builder.addInputMap()` (the set's class
//! extension) registers RayX's input map over the app's key-value store, which the
//! `InputMapService` class reaches (resolving bindings, listing shortcuts, applying the user's
//! document with promises, hearing changes); without a store the start fails naming how to add
//! one. The class, the contract a TS implementation follows (`InputMapServiceContract`) and
//! `addInputMap` with its doc comment are declared.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::element::Registry;
use rayx_app::services::storage::KeyValueStore;
use rayx_app_nsh::testing;

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

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_input_map::register(set);
        set.class_extension::<add_fake_storage>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { InputMapService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addFakeStorage()
      .addInputMap((options) => { options.bindKey("Editor", "cmd-s", "save"); })
      .start();
    const inputMap = services.getRequiredService(InputMapService);
    lines.push(String(inputMap === services.getRequiredService(InputMapService)));
    const save = { kind: "keyboard", value: "Cmd-S" };
    const resolved = inputMap.resolve(["Editor"], save);
    lines.push(`${resolved.action} ${resolved.source} ${inputMap.resolve(["Other"], save)}`);
    lines.push(inputMap.bindingsFor("save").map((entry) => entry.binding.value).join(","));

    const revisions = [];
    const stop = inputMap.onChange((state) => revisions.push(state.revision));
    const applied = await inputMap.applyUserDocument(inputMap.revision(), {
      schema_version: 1, source: "user", revision: 0,
      entries: [{ context: "Editor", binding: { kind: "keyboard", value: "ctrl-s" }, action: "save" }],
    });
    lines.push(`${applied.revision} ${inputMap.revision()} ${inputMap.current().user.entries.length}`);
    lines.push(inputMap.bindingsFor("save").map((entry) => entry.binding.value).join(","));
    try {
      await inputMap.applyUserDocument(0, { schema_version: 1, source: "user", revision: 0 });
      lines.push("applied");
    } catch (error) {
      lines.push(error.message.includes("revision 1, not 0") ? "conflict" : `rejected ${error.message}`);
    }
    lines.push(`${(await inputMap.load()).revision}`);
    await new Promise((resolve) => setTimeout(resolve, 20));
    stop();
    lines.push(revisions.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_input_map_class_reaches_the_service() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|save application null|cmd-s|1 1 1|ctrl-s,cmd-s|conflict|1|1"
    );
}

const WITHOUT_STORE: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
new RayXAppBuilder().addHostFakes().addInputMap().start().then(
  () => shown.set("done started"),
  (error) => shown.set(`done rejected ${error.message}`),
);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn without_a_store_the_start_names_what_to_add() {
    let shown = testing::run(set(), WITHOUT_STORE);
    assert!(shown.starts_with("done rejected"), "{shown}");
    assert!(shown.contains("KeyValueStore"), "{shown}");
    assert!(shown.contains("addStorage"), "{shown}");
}

#[test]
fn the_class_the_contract_and_add_input_map_are_declared() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class InputMapService {",
        "  resolve(contexts: string[], binding: InputBinding): ResolvedInputAction | null;",
        "  bindingsFor(action: string): InputMapEntry[];",
        "  load(): Promise<InputMapDocument>;",
        "  applyUserDocument(expectedRevision: number, candidate: InputMapDocument): Promise<InputMapDocument>;",
        "export interface InputMapServiceContract {",
        "export type InputMapState = ",
        "export type InputMapDocument = ",
        "addInputMap(configure?: ((options: InputMapOptions) => void) | null): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    let add = ts
        .find(
            "addInputMap(configure?: ((options: InputMapOptions) => void) | null): RayXAppBuilder;",
        )
        .unwrap();
    let before = ts[..add].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment on addInputMap in\n{ts}"
    );
    let doc = &before[before.rfind("/**").unwrap()..];
    assert!(doc.contains("input map"), "{doc}");
    assert_eq!(set().undocumented(), Vec::<String>::new());
}

#[test]
fn without_the_set_there_is_no_add_input_map() {
    let ts = testing::declarations(&testing::set(|_| {}));
    assert!(!ts.contains("addInputMap("), "{ts}");
    assert!(!ts.contains("class InputMapService"), "{ts}");
    assert!(!ts.contains("class InputMapOptions"), "{ts}");
}
