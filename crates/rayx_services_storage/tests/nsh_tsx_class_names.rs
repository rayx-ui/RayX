//! The key-value store's TS class in its set: `KeyValueStore`, the contract's token, made by the provider only (a
//! private constructor and no static `of`), with the members the host's class had.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

/// The declaration of class `name` in `ts`.
fn class<'a>(ts: &'a str, name: &str) -> &'a str {
    let head = format!("export declare class {name} {{");
    ts.split(head.as_str())
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class {name} in\n{ts}"))
}

#[test]
fn the_storage_class_carries_its_members() {
    let set = testing::set(rayx_services_storage::register);
    let ts = testing::declarations(&set);
    let declared = class(&ts, "KeyValueStore");
    assert!(declared.contains("private constructor();"), "{declared}");
    assert!(!declared.contains("static of("), "{declared}");
    for member in [
        "get(", "set(", "getJson(", "setJson(", "remove(", "keys(", "clear(",
    ] {
        assert!(
            declared.contains(&format!("  {member}")),
            "KeyValueStore has no `{member}`:\n{declared}"
        );
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
