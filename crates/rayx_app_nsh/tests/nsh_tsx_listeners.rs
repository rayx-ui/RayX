//! Listeners of the host's TS service classes: `ApplicationConfigurationService.onChange` is
//! declared to return the function that stops its listener. (The service crates' classes check
//! theirs in their crates' `nsh_tsx_listeners` groups.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

#[test]
fn listener_members_are_declared_to_return_their_stop_function() {
    let ts = headless::declarations(&headless::set(|_| {}));
    let class = ts
        .split("export declare class ApplicationConfigurationService {")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class ApplicationConfigurationService in\n{ts}"));
    let declared = "  onChange(listener: (settings: unknown) => void): () => void;";
    assert!(class.contains(declared), "no `{declared}` in\n{class}");
}
