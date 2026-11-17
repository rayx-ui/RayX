//! The input map's TS listener: `InputMapService.onChange` is declared to hear the state and to
//! return the function that stops it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx_app_nsh::testing;

#[test]
fn on_change_is_declared_to_return_its_stop_function() {
    let ts = testing::declarations(&testing::set(rayx_services_input_map::register));
    let class = ts
        .split("export declare class InputMapService {")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class InputMapService in\n{ts}"));
    let declared = "  onChange(listener: (state: InputMapState) => void): () => void;";
    assert!(class.contains(declared), "no `{declared}` in\n{class}");
}
