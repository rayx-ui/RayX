//! The storage for TSX through NSH's engine, headless: `builder.addStorage()` (the set's class
//! extension) registers RayX's store, which the `KeyValueStore` class reaches with promises, as
//! text or JSON; the class is declared with its members, `addStorage` with its doc comment, and
//! the contract a TS implementation follows as `KeyValueStoreContract`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::element::Registry;
use rayx_app_nsh::testing;

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_storage::register(set);
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { KeyValueStore, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes().addStorage().start();
    const storage = services.getRequiredService(KeyValueStore);
    lines.push(String(storage === services.getRequiredService(KeyValueStore)));
    lines.push(String(storage.capabilities.persistent));
    await storage.set("prefs", "lang", "hr");
    await storage.setJson("prefs", "theme", { scale: 1.25 });
    lines.push(`${await storage.get("prefs", "lang")} ${(await storage.getJson("prefs", "theme")).scale}`);
    lines.push((await storage.keys("prefs")).join(","));
    lines.push(`${await storage.remove("prefs", "lang")} ${await storage.get("prefs", "lang")}`);
    await storage.clear("prefs");
    lines.push(`${(await storage.keys("prefs")).length} ${await storage.getJson("prefs", "theme")}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_key_value_store_class_reaches_the_store() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|true|hr 1.25|lang,theme|true null|0 null"
    );
}

#[test]
fn the_class_the_contract_and_add_storage_are_declared() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class KeyValueStore {",
        "  get(bucket: string, key: string): Promise<string | null>;",
        "  set(bucket: string, key: string, value: string): Promise<void>;",
        "  getJson(bucket: string, key: string): Promise<unknown>;",
        "  setJson(bucket: string, key: string, value: unknown): Promise<void>;",
        "  remove(bucket: string, key: string): Promise<boolean>;",
        "  keys(bucket: string): Promise<(string)[]>;",
        "  clear(bucket: string): Promise<void>;",
        "export interface KeyValueStoreContract {",
        "addStorage(): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    let add_storage = ts.find("addStorage(): RayXAppBuilder;").unwrap();
    let before = ts[..add_storage].trim_end();
    assert!(
        before.ends_with("*/"),
        "no doc comment on addStorage in\n{ts}"
    );
    let doc = &before[before.rfind("/**").unwrap()..];
    assert!(doc.contains("key-value store"), "{doc}");
    assert_eq!(set().undocumented(), Vec::<String>::new());
}

#[test]
fn without_the_set_there_is_no_add_storage() {
    let ts = testing::declarations(&testing::set(|_| {}));
    assert!(!ts.contains("addStorage(): RayXAppBuilder;"), "{ts}");
    assert!(!ts.contains("class KeyValueStore"), "{ts}");
}
