//! The selection service's TS class in its set: `SelectionService`, the contract's token, made by
//! the provider only (a private constructor and no static `of`), with the service's state and
//! its commands returning promises, and `builder.addSelection()` with its doc comment.

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
fn the_selection_class_carries_its_members() {
    let set = testing::set(rayx_services_selection::register);
    let ts = testing::declarations(&set);
    let declared = class(&ts, "SelectionService");
    assert!(declared.contains("private constructor();"), "{declared}");
    assert!(!declared.contains("static of("), "{declared}");
    for member in [
        "get contract(): SelectionServiceContract | null;",
        "state(): SelectionState;",
        "activeArea(): number | null;",
        "status(): SelectionStatus;",
        "activate(activeArea: number, status: SelectionStatus): Promise<void>;",
        "publishStatus(activeArea: number, status: SelectionStatus): Promise<void>;",
        "deactivate(activeArea: number): Promise<void>;",
        "clear(): Promise<void>;",
        "onChange(",
    ] {
        assert!(
            declared.contains(&format!("  {member}")),
            "SelectionService has no `{member}`:\n{declared}"
        );
    }
    for alias in ["SelectionAreaId", "SelectionStatus", "SelectionState"] {
        assert!(
            ts.contains(&format!("type {alias} =")),
            "no {alias} in\n{ts}"
        );
    }
    assert!(ts.contains("SelectionServiceContract"), "{ts}");
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
fn the_set_declares_add_selection_with_a_doc_comment() {
    let ts = testing::declarations(&testing::set(rayx_services_selection::register));
    let lines: Vec<&str> = ts.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.trim() == "addSelection(): RayXAppBuilder;")
        .unwrap_or_else(|| panic!("no `addSelection(): RayXAppBuilder;` in\n{ts}"));
    assert!(
        lines[at - 1].trim_end().ends_with("*/"),
        "`addSelection` has no doc comment"
    );
    let without = testing::declarations(&testing::set(|_| {}));
    assert!(!without.contains("addSelection("), "{without}");
}
