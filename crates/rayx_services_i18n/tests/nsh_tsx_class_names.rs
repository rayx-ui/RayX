//! The i18n service's TS class in its set: `I18nService`, the contract's token, made by the
//! provider only (a private constructor and no static `of`), with the members the host's class
//! had, its commands returning promises.

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
fn the_i18n_class_carries_its_members() {
    let set = testing::set(rayx_services_i18n::register);
    let ts = testing::declarations(&set);
    let declared = class(&ts, "I18nService");
    assert!(declared.contains("private constructor();"), "{declared}");
    assert!(!declared.contains("static of("), "{declared}");
    for member in [
        "t(",
        "formatDate(",
        "formatNumber(",
        "timeZone(",
        "has(",
        "locale(",
        "languages(",
        "direction(",
        "onChange(",
        "setLanguage(language: string): Promise<void>;",
        "followPlatform(): Promise<void>;",
        "setTimeZone(",
        "loadMessages(): Promise<void>;",
        "addResource(",
    ] {
        assert!(
            declared.contains(&format!("  {member}")),
            "I18nService has no `{member}`:\n{declared}"
        );
    }
    assert!(
        declared
            .lines()
            .filter(|line| line.contains("setTimeZone(") || line.contains("addResource("))
            .all(|line| line.trim_end().ends_with("Promise<void>;")),
        "{declared}"
    );
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
