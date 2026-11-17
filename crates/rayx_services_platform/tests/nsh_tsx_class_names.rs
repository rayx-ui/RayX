//! The platform service's TS class in its set: `PlatformInfoService`, the contract's token, made by the provider only (a
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
fn the_platform_class_carries_its_members() {
    let set = testing::set(rayx_services_platform::register);
    let ts = testing::declarations(&set);
    let declared = class(&ts, "PlatformInfoService");
    assert!(declared.contains("private constructor();"), "{declared}");
    assert!(!declared.contains("static of("), "{declared}");
    for member in ["current(", "onChange("] {
        assert!(
            declared.contains(&format!("  {member}")),
            "PlatformInfoService has no `{member}`:\n{declared}"
        );
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
