//! The history's TS class in its set: `HistoryService` (the contract's token, made by the
//! provider only), the contract a TS implementation follows, its options class `HistoryOptions`,
//! and `addHistory` and `addPersistentHistory` (each with a doc comment and its options
//! callback) only once the set is registered.

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
fn the_history_class_carries_its_members() {
    let set = testing::set(rayx_services_history::register);
    let ts = testing::declarations(&set);
    let declared = class(&ts, "HistoryService");
    // Made by the provider only: never `new`ed or `of(app)`.
    assert!(declared.contains("private constructor();"), "{declared}");
    assert!(!declared.contains("static of("), "{declared}");
    for member in [
        "get contract(): HistoryServiceContract | null;",
        "canUndo(scopeId: string): boolean;",
        "canRedo(scopeId: string): boolean;",
        "depths(scopeId: string): (number)[];",
        "bindTargetScope(targetId: string, scopeId: string): Promise<void>;",
        "undo(scopeId: string): Promise<boolean>;",
        "redo(scopeId: string): Promise<boolean>;",
        "clearScope(scopeId: string): Promise<void>;",
        "setScopeEntryLimit(scopeId: string, limit: number): Promise<void>;",
        "retainedRecords(scopeId: string): Promise<OperationRecord[]>;",
        "onChange(listener: (event: HistoryEvent) => void): () => void;",
    ] {
        assert!(
            declared.contains(&format!("  {member}")),
            "HistoryService has no `{member}`:\n{declared}"
        );
    }
    for declared in [
        "export interface HistoryServiceContract {",
        "  undo(scopeId: string): Promise<boolean>;",
        "  retainedRecords(scopeId: string): Promise<OperationRecord[]>;",
        "  readonly history: ContractState<HistoryState>;",
        "export type HistoryState = ",
        "export type HistoryEvent = ",
        "export type OperationRecord = ",
        "export declare class HistoryOptions {",
        "  callTimeoutMs(timeout?: number | null): HistoryOptions;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
fn the_builder_methods_exist_only_with_the_set() {
    let without = testing::declarations(&testing::set(|_| {}));
    assert!(!without.contains("addHistory("), "{without}");
    assert!(!without.contains("addPersistentHistory("), "{without}");
    assert!(!without.contains("class HistoryService"), "{without}");
    let with = testing::declarations(&testing::set(rayx_services_history::register));
    let builder = class(&with, "RayXAppBuilder");
    let lines: Vec<&str> = builder.lines().collect();
    for method in [
        "addHistory(configure?: ((options: HistoryOptions) => void) | null): RayXAppBuilder;",
        "addPersistentHistory(configure?: ((options: HistoryOptions) => void) | null): RayXAppBuilder;",
    ] {
        let at = lines
            .iter()
            .position(|line| line.trim() == method)
            .unwrap_or_else(|| panic!("no `{method}` in\n{builder}"));
        assert!(
            lines[at - 1].trim_end().ends_with("*/"),
            "`{method}` has no doc comment"
        );
    }
}
