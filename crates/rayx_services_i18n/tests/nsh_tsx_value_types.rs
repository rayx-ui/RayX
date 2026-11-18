//! The i18n service's TS class declares `t`'s arguments as the object of text, numbers, booleans
//! and instants it takes, not `unknown`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

/// The declaration of the class `name` in `ts`, up to its closing brace.
fn class<'a>(ts: &'a str, name: &str) -> &'a str {
    ts.split(&format!("export declare class {name} {{"))
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class {name} in\n{ts}"))
}

#[test]
fn t_s_arguments_are_declared_as_an_object_of_values() {
    let ts = testing::declarations(&testing::set(rayx_services_i18n::register));
    let declared = class(&ts, "I18nService");
    let line =
        "  t(id: string, args?: Record<string, string | number | boolean | null> | null): string;";
    assert!(declared.contains(line), "no `{line}` in\n{declared}");
    assert!(
        !ts.contains("(): unknown"),
        "a member still gives `unknown` in\n{ts}"
    );
}
