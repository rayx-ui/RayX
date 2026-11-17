//! The navigation's TS classes in its set: `NavigationService` (the contract's token, made by
//! the provider only) and `NavigationRegion` with the members the host's classes had, the
//! contract a TS implementation follows, and `addNavigation` only once the set is registered.

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
fn the_navigation_classes_carry_their_members() {
    let set = testing::set(rayx_services_navigation::register);
    let ts = testing::declarations(&set);
    for (name, members) in [
        (
            "NavigationService",
            &[
                "navigate(",
                "back(",
                "forward(",
                "current(",
                "region(name: string): NavigationRegion;",
                "onChange(",
                "get contract(): NavigationServiceContract | null;",
            ][..],
        ),
        (
            "NavigationRegion",
            &[
                "navigate(",
                "back(",
                "forward(",
                "goTo(",
                "close(",
                "pages(",
                "onChange(",
            ],
        ),
    ] {
        let declared = class(&ts, name);
        assert!(!declared.contains("static of("), "{name}:\n{declared}");
        for member in members {
            assert!(
                declared.contains(&format!("  {member}")),
                "{name} has no `{member}`:\n{declared}"
            );
        }
    }
    assert!(
        class(&ts, "NavigationService").contains("private constructor();"),
        "{ts}"
    );
    for declared in [
        "export interface NavigationServiceContract {",
        "  navigate(request: NavigationRequest): NavigationResult;",
        "  goToEntry(region: string, index: number): NavigationResult;",
        "  applyLocation(location: string): NavigationResult[];",
        "  readonly defaultHistoryMode: NavigationHistoryMode;",
        "export type NavigationState = ",
        "export type NavigationEvent = ",
        "export type NavigationHistoryMode = ",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
fn add_navigation_exists_only_with_the_set() {
    let without = testing::declarations(&testing::set(|_| {}));
    assert!(!without.contains("addNavigation("), "{without}");
    assert!(!without.contains("class NavigationService"), "{without}");
    let with = testing::declarations(&testing::set(rayx_services_navigation::register));
    assert!(with.contains("addNavigation("), "{with}");
}
