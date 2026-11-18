//! The operation service's TS classes in its set: `OperationService` (the contract's token,
//! made by the provider only) and `OperationTarget` (a target's handler, taken from a handle),
//! the contract a TS implementation follows, and `addOperations` only once the set is
//! registered.

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
fn the_operation_classes_carry_their_members() {
    let set = testing::set(rayx_services_operations::register);
    let ts = testing::declarations(&set);
    let declared = class(&ts, "OperationService");
    // Made by the provider only: never `new`ed or `of(app)`.
    assert!(declared.contains("private constructor();"), "{declared}");
    assert!(!declared.contains("static of("), "{declared}");
    for member in [
        "get contract(): OperationServiceContract | null;",
        "get instanceId(): number;",
        "get targets(): (string)[];",
        "registerTarget(targetId: string, target: OperationTarget): Promise<void>;",
        "releaseTarget(targetId: string): Promise<boolean>;",
        "begin(): Promise<number>;",
        "commit(transactionId: number): Promise<void>;",
        "abort(transactionId: number): Promise<void>;",
        "submitRecord(record: OperationRecord, origin: OperationOrigin, transactionId?: number | null): Promise<OperationReceipt>;",
        "publishRemoteRecord(",
        "onEvent(",
    ] {
        assert!(
            declared.contains(&format!("  {member}")),
            "OperationService has no `{member}`:\n{declared}"
        );
    }
    let target = class(&ts, "OperationTarget");
    assert!(target.contains("private constructor();"), "{target}");
    for member in [
        "static of(handle: OperationTargetHandle): OperationTarget;",
        "apply(record: OperationRecord, origin: OperationOrigin): OperationTargetResult;",
    ] {
        assert!(
            target.contains(&format!("  {member}")),
            "OperationTarget has no `{member}`:\n{target}"
        );
    }
    for declared in [
        "export interface OperationServiceContract {",
        "  registerTarget(targetId: string, handler: OperationTargetHandle): void;",
        "  submitRecord(record: OperationRecord, origin: OperationOrigin, transactionId: number | null): Promise<OperationReceipt>;",
        "  readonly instanceId: number;",
        "export type OperationState = ",
        "export type OperationEvent = ",
        "export type OperationRecord = ",
        "export type OperationTargetHandle = ",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
fn add_operations_exists_only_with_the_set() {
    let without = testing::declarations(&testing::set(|_| {}));
    assert!(!without.contains("addOperations("), "{without}");
    assert!(!without.contains("class OperationService"), "{without}");
    let with = testing::declarations(&testing::set(rayx_services_operations::register));
    let builder = class(&with, "RayXAppBuilder");
    let lines: Vec<&str> = builder.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.trim() == "addOperations(): RayXAppBuilder;")
        .unwrap_or_else(|| panic!("no addOperations in\n{with}"));
    assert!(
        lines[at - 1].trim_end().ends_with("*/"),
        "addOperations has no doc comment"
    );
}
