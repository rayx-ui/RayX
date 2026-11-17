//! The TS classes of RayX's services carry the Rust services' names, with the members the old
//! classes had and no static `of`; the old entry and scope classes and the old names are gone
//! from the set and its declarations. (The service crates' classes, `AssetService` and the
//! others, are in their crates' sets.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

/// The declaration of class `name` in `ts`.
fn class<'a>(ts: &'a str, name: &str) -> &'a str {
    let head = format!("export declare class {name} {{");
    ts.split(head.as_str())
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap_or_else(|| panic!("no class {name} in\n{ts}"))
}

#[test]
fn the_service_classes_carry_the_rust_services_names_and_their_members() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for (name, members) in [
        (
            "I18nService",
            &[
                "t(",
                "formatDate(",
                "formatNumber(",
                "timeZone(",
                "has(",
                "locale(",
                "languages(",
                "direction(",
                "setLanguage(",
                "onChange(",
            ][..],
        ),
        (
            "NavigationService",
            &[
                "navigate(",
                "back(",
                "forward(",
                "current(",
                "region(name: string): NavigationRegion;",
                "onChange(",
            ],
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
        (
            "KeyValueStore",
            &[
                "get(", "set(", "getJson(", "setJson(", "remove(", "keys(", "clear(",
            ],
        ),
        (
            "ApplicationConfigurationService",
            &[
                "current(",
                "layoutDirection(",
                "reducedMotion(",
                "onChange(",
            ],
        ),
    ] {
        let declared = class(&ts, name);
        // Made by the provider only: its own token, never `new`ed or `of(app)`.
        assert!(
            declared.contains("private constructor();"),
            "{name}:\n{declared}"
        );
        assert!(!declared.contains("static of("), "{name}:\n{declared}");
        for member in members {
            assert!(
                declared.contains(&format!("  {member}")),
                "{name} has no `{member}`:\n{declared}"
            );
        }
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
fn the_old_api_is_gone_from_the_set() {
    let set = headless::set(|_| {});
    let ts = headless::declarations(&set);
    for old in [
        concat!("App", "Services"),
        concat!("Service", "Scope"),
        "class I18n ",
        "class Navigation ",
        "class Storage ",
        "class AppConfiguration ",
        "class Assets ",
        "class Fonts ",
        "class Platform ",
        "static of(",
        "load(",
    ] {
        assert!(!ts.contains(old), "`{old}` is still declared:\n{ts}");
    }
    let names: Vec<&str> = set.class_specs().iter().map(|class| class.name).collect();
    for name in [
        "RayXAppBuilder",
        "ServiceProvider",
        "ServiceToken",
        "I18nService",
        "NavigationService",
        "NavigationRegion",
        "KeyValueStore",
        "ApplicationConfigurationService",
    ] {
        assert!(names.contains(&name), "{name} is not in the set: {names:?}");
    }
}
