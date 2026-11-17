//! Listeners of a service's TS class built on the host's `Listeners` (the tests' own
//! `GreetingService` here): `onChange` is declared to return the function that stops its
//! listener. (The service crates' classes check theirs in their crates' `nsh_tsx_listeners`
//! groups.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

#[test]
fn listener_members_are_declared_to_return_their_stop_function() {
    let ts = headless::declarations(&headless::set(headless::greeting::register));
    let class = ts
        .split("export declare class GreetingService {")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class GreetingService in\n{ts}"));
    let declared = "  onChange(listener: (greeting: string) => void): () => void;";
    assert!(class.contains(declared), "no `{declared}` in\n{class}");
}
