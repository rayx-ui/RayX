//! The navigation service: routes, journals, regions, lifecycle subscribers and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use rayx_app::services::navigation::NavigationService;
use rayx_services_navigation::testing::{self, NoopNavigationLocationSynchronizer};
use rayx_services_navigation::{NavigationOptions, RouteDefinition};

/// RayX's navigation over `patterns`, showing the location nowhere.
fn navigation(patterns: &[&str]) -> Arc<dyn NavigationService> {
    with_options(patterns, NavigationOptions::default())
}

fn with_options(patterns: &[&str], mut options: NavigationOptions) -> Arc<dyn NavigationService> {
    for pattern in patterns {
        options.add_route(RouteDefinition::new(*pattern));
    }
    testing::navigation(options, Arc::new(NoopNavigationLocationSynchronizer)).unwrap()
}

mod lifecycle {
    use std::sync::{Arc, Mutex};

    use futures::executor::block_on;
    use rayx_app::services::navigation::{
        NavigationArgs, NavigationDirection, NavigationRequest, RoutingStatus,
    };
    use serde_json::json;

    use super::navigation;

    #[test]
    fn lifecycle_callbacks_publish_complete_data_in_contract_order() {
        let navigation = navigation(&["/a", "/b"]);
        let events = Arc::new(Mutex::new(Vec::new()));

        let from_events = Arc::clone(&events);
        let _from = navigation.on_navigated_from(move |data| {
            from_events.lock().unwrap().push((
                "from",
                data.direction,
                data.from.as_ref().map(|value| value.route.location.clone()),
                data.to.route.location.clone(),
                data.to.route.passed_parameters.clone(),
            ));
        });
        let to_events = Arc::clone(&events);
        let _to = navigation.on_navigated_to(move |data| {
            to_events.lock().unwrap().push((
                "to",
                data.direction,
                data.from.as_ref().map(|value| value.route.location.clone()),
                data.to.route.location.clone(),
                data.to.route.passed_parameters.clone(),
            ));
        });

        block_on(navigation.navigate(NavigationRequest {
            region: "root".to_string(),
            args: NavigationArgs {
                parameters: Some(json!({"source": "initial"})),
                ..NavigationArgs::new("/a")
            },
        }))
        .unwrap();
        block_on(navigation.navigate_to("/b")).unwrap();
        block_on(navigation.back("root".into())).unwrap();
        block_on(navigation.forward("root".into())).unwrap();
        block_on(navigation.refresh("root".into())).unwrap();

        assert_eq!(
            events.lock().unwrap().as_slice(),
            [
                (
                    "to",
                    NavigationDirection::New,
                    None,
                    "/a".to_string(),
                    Some(json!({"source": "initial"})),
                ),
                (
                    "from",
                    NavigationDirection::New,
                    Some("/a".to_string()),
                    "/b".to_string(),
                    None,
                ),
                (
                    "to",
                    NavigationDirection::New,
                    Some("/a".to_string()),
                    "/b".to_string(),
                    None,
                ),
                (
                    "from",
                    NavigationDirection::Back,
                    Some("/b".to_string()),
                    "/a".to_string(),
                    Some(json!({"source": "initial"})),
                ),
                (
                    "to",
                    NavigationDirection::Back,
                    Some("/b".to_string()),
                    "/a".to_string(),
                    Some(json!({"source": "initial"})),
                ),
                (
                    "from",
                    NavigationDirection::Forward,
                    Some("/a".to_string()),
                    "/b".to_string(),
                    None,
                ),
                (
                    "to",
                    NavigationDirection::Forward,
                    Some("/a".to_string()),
                    "/b".to_string(),
                    None,
                ),
                (
                    "from",
                    NavigationDirection::Refresh,
                    Some("/b".to_string()),
                    "/b".to_string(),
                    None,
                ),
                (
                    "to",
                    NavigationDirection::Refresh,
                    Some("/b".to_string()),
                    "/b".to_string(),
                    None,
                ),
            ]
        );

        let before_unsuccessful = events.lock().unwrap().len();
        assert_eq!(
            block_on(navigation.navigate_to("/b")).unwrap().status,
            RoutingStatus::SkippedSameRoute
        );
        assert_ne!(
            block_on(navigation.navigate_to("/missing")).unwrap().status,
            RoutingStatus::Success
        );
        assert_ne!(
            block_on(navigation.navigate_to("https://external.example/"))
                .unwrap()
                .status,
            RoutingStatus::Success
        );
        assert_eq!(events.lock().unwrap().len(), before_unsuccessful);
    }

    #[test]
    fn lifecycle_callbacks_release_state_and_ignore_no_history() {
        let navigation = navigation(&["/a", "/b"]);
        let order = Arc::new(Mutex::new(Vec::new()));

        let from_order = Arc::clone(&order);
        let navigation_from = Arc::clone(&navigation);
        let _from = navigation.on_navigated_from(move |_| {
            from_order.lock().unwrap().push("from");
            assert_eq!(navigation_from.current().unwrap().route.location, "/b");
        });
        let to_order = Arc::clone(&order);
        let _to = navigation.on_navigated_to(move |_| to_order.lock().unwrap().push("to"));
        let route_order = Arc::clone(&order);
        let _route =
            navigation.on_route_changed(move |_| route_order.lock().unwrap().push("route"));
        let location_order = Arc::clone(&order);
        let _location = navigation
            .on_location_changed(move |_| location_order.lock().unwrap().push("location"));

        block_on(navigation.navigate_to("/a")).unwrap();
        order.lock().unwrap().clear();
        block_on(navigation.navigate_to("/b")).unwrap();
        assert_eq!(
            order.lock().unwrap().as_slice(),
            ["from", "to", "route", "location"]
        );

        let no_history = super::navigation(&["/"]);
        let callback_count = Arc::new(Mutex::new(0usize));
        let observed = Arc::clone(&callback_count);
        let _to = no_history.on_navigated_to(move |_| *observed.lock().unwrap() += 1);

        assert_eq!(
            block_on(no_history.back("root".into())).unwrap().status,
            RoutingStatus::SkippedNoHistory
        );
        assert_eq!(
            block_on(no_history.refresh("root".into())).unwrap().status,
            RoutingStatus::SkippedNoHistory
        );
        assert_eq!(*callback_count.lock().unwrap(), 0);
    }
}

mod service {
    use std::sync::{Arc, Mutex};

    use futures::executor::block_on;
    use rayx_app::services::navigation::*;
    use rayx_services_navigation::testing::{self, NavigationLocationSynchronizer};
    use rayx_services_navigation::{NavigationOptions, RouteDefinition, RouteMap};

    use super::{navigation, with_options};

    #[test]
    fn route_params_match_braced_segments() {
        let mut routes = RouteMap::default();
        routes.route("/projects/{project}").finish();
        let matched = routes.process_route("/projects/rayx", None).unwrap();
        assert_eq!(matched.status, RoutingStatus::Success);
        assert_eq!(matched.route.params.get("project"), Some("rayx"));
    }

    #[test]
    fn literal_routes_win_over_parameters_and_registration_breaks_ties() {
        let mut routes = RouteMap::default();
        routes
            .route("/files/{name}")
            .component("parameter")
            .finish();
        routes
            .route("/files/settings")
            .component("literal")
            .finish();
        routes.route("/{*path}").component("catch_all").finish();

        assert_eq!(
            routes
                .process_route("/files/settings", None)
                .unwrap()
                .route
                .component
                .as_deref(),
            Some("literal")
        );
        assert_eq!(
            routes
                .process_route("/files/readme", None)
                .unwrap()
                .route
                .component
                .as_deref(),
            Some("parameter")
        );
        assert_eq!(
            routes
                .process_route("/other/path", None)
                .unwrap()
                .route
                .params
                .get("path"),
            Some("other/path")
        );
    }

    #[test]
    fn route_parses_query_fragment_and_external_status() {
        let mut routes = RouteMap::new("https://example.com/app/");
        routes.route("/search").finish();
        let matched = routes
            .process_route("/search?q=rayx#top", None)
            .expect("route should parse");
        assert_eq!(matched.route.location, "/search?q=rayx#top");
        assert_eq!(matched.route.query_params.get("q"), Some("rayx"));

        let external = routes
            .process_route("https://external.example/search", None)
            .expect("external route should parse");
        assert_eq!(external.status, RoutingStatus::SkippedExternalRoute);
    }

    #[test]
    fn journal_modes_follow_push_replace_reset_none_rules() {
        let mut routes = RouteMap::default();
        routes.route("/a").finish();
        routes.route("/b").finish();
        routes.route("/c").finish();
        let a = routes.process_route("/a", None).unwrap().route;
        let b = routes.process_route("/b", None).unwrap().route;
        let c = routes.process_route("/c", None).unwrap().route;
        let mut journal = NavigationJournal::new(2);

        journal.record(a.clone(), NavigationHistoryMode::Push);
        journal.record(b.clone(), NavigationHistoryMode::Push);
        journal.record(c.clone(), NavigationHistoryMode::Push);
        assert_eq!(journal.count(), 2);
        assert_eq!(journal.previous().unwrap().location, "/b");
        assert_eq!(journal.current().unwrap().location, "/c");

        let previous = journal.try_go_back().unwrap();
        assert_eq!(previous.location, "/b");
        journal.record(a, NavigationHistoryMode::Replace);
        assert_eq!(journal.current().unwrap().location, "/a");
        journal.record(c, NavigationHistoryMode::None);
        assert_eq!(journal.current().unwrap().location, "/a");
    }

    #[test]
    fn navigation_updates_root_location_and_synchronizes_location()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        #[derive(Default)]
        struct Recorder {
            calls: Mutex<Vec<(String, NavigationHistoryMode)>>,
        }

        impl NavigationLocationSynchronizer for Recorder {
            fn synchronize(&self, location: &str, mode: NavigationHistoryMode) -> Result<()> {
                self.calls
                    .lock()
                    .unwrap()
                    .push((location.to_string(), mode));
                Ok(())
            }
        }

        let mut options = NavigationOptions::default();
        options.add_route(RouteDefinition::new("/projects/{project}"));
        let recorder = Arc::new(Recorder::default());
        let navigation = testing::navigation(options, recorder.clone())?;
        let result = block_on(navigation.navigate(NavigationRequest {
            region: "root".to_string(),
            args: NavigationArgs::new("/projects/rayx"),
        }))?;
        assert_eq!(result.route.params.get("project"), Some("rayx"));
        assert_eq!(
            recorder.calls.lock().unwrap().as_slice(),
            &[("/projects/rayx".to_string(), NavigationHistoryMode::Push)]
        );
        Ok(())
    }

    #[test]
    fn named_regions_have_independent_journals_and_root_is_protected() {
        let navigation = navigation(&["/home", "/tool"]);

        block_on(navigation.navigate_to("/home")).unwrap();
        block_on(navigation.navigate_in_region("sidebar", NavigationArgs::new("/tool"))).unwrap();

        assert_eq!(
            navigation.current_in_region("root").unwrap().route.location,
            "/home"
        );
        assert_eq!(
            navigation
                .current_in_region("sidebar")
                .unwrap()
                .route
                .location,
            "/tool"
        );
        assert_eq!(navigation.region_names(), ["root", "sidebar"]);
        assert!(!block_on(navigation.remove_region("root".into())).unwrap());
        assert!(block_on(navigation.remove_region("sidebar".into())).unwrap());
        assert_eq!(navigation.region_names(), ["root"]);
    }

    #[test]
    fn a_region_is_made_once() {
        let navigation = navigation(&["/a"]);
        let published = Arc::new(Mutex::new(0usize));
        let seen = Arc::clone(&published);
        let _published = navigation
            .navigation()
            .subscribe(move |_| *seen.lock().unwrap() += 1);
        block_on(navigation.ensure_region("panel".into())).unwrap();
        block_on(navigation.ensure_region("panel".into())).unwrap();
        assert_eq!(*published.lock().unwrap(), 1);
        let panel = navigation.region("panel").unwrap();
        assert_eq!(panel.journal.count(), 0);
        assert!(panel.current.is_none());
    }

    #[test]
    fn failed_routes_can_use_configured_not_found_fallback() {
        let navigation = with_options(
            &["/not-found"],
            NavigationOptions {
                not_found_route: Some("/not-found".to_string()),
                ..NavigationOptions::default()
            },
        );

        let result = block_on(navigation.navigate_to("/missing")).unwrap();
        assert_eq!(result.status, RoutingStatus::Success);
        assert_eq!(result.route.location, "/not-found");
    }

    #[test]
    fn navigation_lifecycle_subscribers_ignore_unsuccessful_outcomes() {
        let navigation = navigation(&["/a", "/b"]);
        let events = Arc::new(Mutex::new(0usize));
        let observed = Arc::clone(&events);
        let _subscription = navigation.on_navigated_to(move |_| *observed.lock().unwrap() += 1);

        block_on(navigation.navigate_to("/a")).unwrap();
        assert_eq!(*events.lock().unwrap(), 1);
        block_on(navigation.navigate_to("/a")).unwrap();
        block_on(navigation.navigate_to("/missing")).unwrap();
        block_on(navigation.navigate_to("https://external.example/")).unwrap();
        block_on(navigation.back("root".into())).unwrap();
        block_on(navigation.back("root".into())).unwrap();

        assert_eq!(*events.lock().unwrap(), 1);
    }

    #[test]
    fn the_state_is_published_before_the_events_and_carries_the_constants() {
        let navigation = navigation(&["/a"]);
        assert_eq!(navigation.root_region_name(), "root");
        assert_eq!(navigation.url_region(), "root");
        assert_eq!(
            navigation.default_history_mode(),
            NavigationHistoryMode::Push
        );
        assert_eq!(navigation.navigation().get().root_region, "root");
        let source = Arc::clone(&navigation);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::clone(&seen);
        let _events = navigation.events().subscribe(move |event| {
            if let NavigationEvent::RouteChanged(changed) = event {
                heard
                    .lock()
                    .unwrap()
                    .push((changed.to.location.clone(), source.location()));
            }
        });
        block_on(navigation.navigate_to("/a")).unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            [("/a".to_string(), "/a".to_string())]
        );
    }
}

mod registration {
    use rayx_app::services::navigation::NavigationService;
    use rayx_app::{AppBuilder as _, RayXAppBuilder};
    use rayx_services_navigation::{NavigationAppBuilderExt, NavigationOptions, RouteDefinition};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn add_navigation_binds_the_section_then_configures_in_code()
    -> Result<(), Box<dyn std::error::Error>> {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&observed);
        let mut builder = RayXAppBuilder::new().add_navigation(move |options| {
            seen.lock().unwrap().push(options.clone());
            options.max_journal_entries = 10;
            options.root_region_name = "main".to_string();
            options.add_route(RouteDefinition::new("/home"));
        });
        builder.configuration.add_value(json!({
            "RayX": { "Services": { "Navigation": { "defaultRoute": "/configured", "maxJournalEntries": 72 } } }
        }));
        builder.configuration.add_value(json!({
            "RayX": { "Services": { "Navigation": { "defaultRoute": "/layered" } } }
        }));
        let services = builder.build()?;
        services
            .validate_options_on_start()
            .map_err(|errors| format!("{errors:?}"))?;
        let options = services.options::<NavigationOptions>()?;
        let navigation = services.required_service::<dyn NavigationService>()?;

        let observed = observed.lock().unwrap();
        assert_eq!(observed[0].default_route, "/layered");
        assert_eq!(observed[0].max_journal_entries, 72);
        assert_eq!(options.value().max_journal_entries, 10);
        assert_eq!(options.value().routes.len(), 1);
        // The service is made from them.
        assert_eq!(navigation.root_region_name(), "main");
        assert!(services.has_startup_role::<dyn NavigationService>());
        Ok(())
    }

    #[test]
    fn a_navigation_service_registered_after_it_wins() -> Result<(), Box<dyn std::error::Error>> {
        let mut builder = RayXAppBuilder::new()
            .add_navigation(|options| options.default_route = "/default".to_string());
        let custom: Arc<dyn NavigationService> = rayx_app::testing::FakeNavigation::new();
        builder
            .services_mut()
            .replace_service::<dyn NavigationService>(Arc::clone(&custom));
        let services = builder.build()?;
        let navigation = services.required_service::<dyn NavigationService>()?;
        assert!(Arc::ptr_eq(&navigation, &custom));
        Ok(())
    }

    #[test]
    fn add_navigation_replaces_a_navigation_service_registered_before_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut builder = RayXAppBuilder::new();
        let custom: Arc<dyn NavigationService> = rayx_app::testing::FakeNavigation::new();
        builder
            .services_mut()
            .replace_service::<dyn NavigationService>(Arc::clone(&custom));
        let services = builder
            .add_navigation(|options| options.root_region_name = "main".to_string())
            .build()?;
        let navigation = services.required_service::<dyn NavigationService>()?;
        assert!(!Arc::ptr_eq(&navigation, &custom));
        assert_eq!(navigation.root_region_name(), "main");
        Ok(())
    }

    #[test]
    fn invalid_routes_fail_on_start() -> Result<(), Box<dyn std::error::Error>> {
        let services = RayXAppBuilder::new()
            .add_navigation(|options| {
                options.add_route(RouteDefinition::new("/a"));
                options.add_route(RouteDefinition::new("/a"));
            })
            .build()?;
        assert!(services.validate_options_on_start().is_err());
        Ok(())
    }
}
