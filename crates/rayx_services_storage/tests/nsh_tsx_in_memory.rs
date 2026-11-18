//! `builder.addInMemoryStorage()` in TSX, headless: the app's key-value store kept in memory
//! (not persistent), reached through the `KeyValueStore` class; it replaces a store registered
//! before it, and is declared with its doc comment.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nsh_core::element::Registry;
use rayx_app_nsh::testing;

fn set() -> Registry {
    testing::set(rayx_services_storage::register)
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { KeyValueStore, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addStorage().addInMemoryStorage().start();
    const storage = services.getRequiredService(KeyValueStore);
    lines.push(String(storage.capabilities.persistent));
    await storage.set("prefs", "lang", "hr");
    await storage.setJson("prefs", "theme", { scale: 1.25 });
    lines.push(`${await storage.get("prefs", "lang")} ${(await storage.getJson("prefs", "theme")).scale}`);
    lines.push((await storage.keys("prefs")).join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn add_in_memory_storage_keeps_the_values_in_memory() {
    assert_eq!(testing::run(set(), TSX), "done false|hr 1.25|lang,theme");
}

#[test]
fn add_in_memory_storage_is_declared() {
    let ts = testing::declarations(&set());
    let declared = ts
        .find("addInMemoryStorage(): RayXAppBuilder;")
        .unwrap_or_else(|| panic!("no addInMemoryStorage in\n{ts}"));
    assert!(
        ts[..declared].trim_end().ends_with("*/"),
        "no doc comment on addInMemoryStorage in\n{ts}"
    );
}
