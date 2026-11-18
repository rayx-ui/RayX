//! The key-value store's contract declares each command answering a value as a promise of it,
//! so a TS implementation's `async` methods type-check against `KeyValueStoreContract`; a
//! command answering nothing stays `void`, which a promise of nothing satisfies.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

#[test]
fn the_contract_s_commands_answer_promises() {
    let ts = testing::declarations(&testing::set(rayx_services_storage::register));
    for declared in [
        "  get(bucket: string, key: string): Promise<number[] | Uint8Array | null>;",
        "  remove(bucket: string, key: string): Promise<boolean>;",
        "  keys(bucket: string): Promise<string[]>;",
        "  set(bucket: string, key: string, value: number[]): void;",
        "  clear(bucket: string): void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
