//! The navigation's TS classes declare the routes they give as the types they are (the JSON of
//! Rust's `Route`, `RoutingData` and `RouteChangedEvent`), not `unknown`: the service's
//! `current()` and `onChange`'s event, a region's `pages()` and its `onChange`'s pages, and a
//! navigation's result.

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
fn routes_are_declared_as_navigation_routes() {
    let ts = testing::declarations(&testing::set(rayx_services_navigation::register));
    let declared = class(&ts, "NavigationService");
    for line in [
        "  current(): RoutingData | null;",
        "  onChange(listener: (event: RouteChangedEvent) => void): () => void;",
    ] {
        assert!(declared.contains(line), "no `{line}` in\n{declared}");
    }
    let declared = class(&ts, "NavigationRegion");
    for line in [
        "  pages(): NavigationRoute[];",
        "  onChange(listener: (pages: NavigationRoute[]) => void): () => void;",
    ] {
        assert!(declared.contains(line), "no `{line}` in\n{declared}");
    }
    for alias in [
        r#"export type NavigationRoute = { uri: string; base_uri: string; path: string; query: string; fragment: string; location: string; segments: string[]; params: Record<string, string>; query_params: Record<string, string>; passed_parameters: unknown; component: string | null; pattern: string; prewarm: boolean };"#,
        r#"export type RoutingData = { status: RoutingStatus; route: NavigationRoute };"#,
        r#"export type RoutingStatus = "None" | "Success" | "FailedNotFound" | "ComponentNotFound" | "SkippedSameRoute" | "SkippedExternalRoute" | "SkippedNoHistory" | "Cancelled";"#,
        r#"export type NavigationDirection = "New" | "Back" | "Forward" | "Refresh";"#,
        r#"export type RouteChangedEvent = { region: string; from: NavigationRoute | null; to: NavigationRoute };"#,
        r#"export type NavigationResult = { region: string; direction: NavigationDirection; status: RoutingStatus; route: NavigationRoute };"#,
    ] {
        assert!(ts.contains(alias), "no `{alias}` in\n{ts}");
    }
    assert!(
        !ts.contains("(): unknown"),
        "a member still gives `unknown` in\n{ts}"
    );
}

/// The keys of the object type `ts` (`{ a: string; b?: number }`), at its top level.
fn keys(ts: &str) -> Vec<String> {
    let body = ts.trim().trim_start_matches('{').trim_end_matches('}');
    body.split(';')
        .filter_map(|field| field.split(':').next())
        .map(|key| key.trim().trim_end_matches('?').to_owned())
        .filter(|key| !key.is_empty())
        .collect()
}

#[test]
fn the_route_types_have_the_keys_of_rust_s_json() {
    use rayx_app::services::navigation::{RouteChangedEvent, RoutingData, RoutingStatus};
    let route =
        rayx_services_navigation::parse_route("/projects/12?tab=files#top", "/", None).unwrap();
    let json_keys = |value: serde_json::Value| {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    let sorted = |mut keys: Vec<String>| {
        keys.sort();
        keys
    };
    for (alias, value) in [
        (
            rayx_services_navigation::NAVIGATION_ROUTE,
            serde_json::to_value(&route).unwrap(),
        ),
        (
            rayx_services_navigation::ROUTING_DATA,
            serde_json::to_value(RoutingData {
                status: RoutingStatus::Success,
                route: route.clone(),
            })
            .unwrap(),
        ),
        (
            rayx_services_navigation::ROUTE_CHANGED_EVENT,
            serde_json::to_value(RouteChangedEvent {
                region: "root".into(),
                from: None,
                to: route.clone(),
            })
            .unwrap(),
        ),
    ] {
        assert_eq!(sorted(keys(alias.ts)), json_keys(value), "{}", alias.name);
    }
}
