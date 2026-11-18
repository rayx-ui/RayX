//! The suspension service's TS class declares the event `onLifecycle` passes as a
//! `SuspensionEvent`, and `askBeforeQuit`'s listener as taking nothing, not `unknown`.

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
fn lifecycle_listeners_are_declared_with_their_values() {
    let ts = testing::declarations(&testing::set(rayx_services_suspension::register));
    let declared = class(&ts, "SuspensionService");
    for line in [
        "  onLifecycle(listener: (event: SuspensionEvent) => void): () => void;",
        "  askBeforeQuit(listener: () => void): () => void;",
    ] {
        assert!(declared.contains(line), "no `{line}` in\n{declared}");
    }
    assert!(
        !ts.contains("(): unknown"),
        "a member still gives `unknown` in\n{ts}"
    );
}
