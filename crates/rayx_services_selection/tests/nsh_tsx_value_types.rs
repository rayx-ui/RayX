//! The selection service's TS class declares the selection `onChange` passes as the
//! `SelectionState` `state()` gives, not `unknown`.

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
fn on_change_s_selection_is_declared_as_selection_state() {
    let ts = testing::declarations(&testing::set(rayx_services_selection::register));
    let declared = class(&ts, "SelectionService");
    let line = "  onChange(listener: (state: SelectionState) => void): () => void;";
    assert!(declared.contains(line), "no `{line}` in\n{declared}");
    assert!(
        !ts.contains("(): unknown"),
        "a member still gives `unknown` in\n{ts}"
    );
}
