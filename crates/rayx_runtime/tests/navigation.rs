//! The navigation service: routes, journals, regions, lifecycle subscribers and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod lifecycle {
    use std::sync::{Arc, Mutex};

    use rayx_runtime::services::navigation::{
        NavigationArgs, NavigationDirection, NavigationRequest, NavigationService,
        NoopNavigationLocationSynchronizer, RouteMap, RoutingStatus,
    };
    use serde_json::json;

    #[test]
    fn lifecycle_callbacks_publish_complete_data_in_contract_order() {
        let mut routes = RouteMap::default();
        routes.route("/a").finish();
        routes.route("/b").finish();
        let navigation = NavigationService::new(routes)
            .with_location_synchronizer(Arc::new(NoopNavigationLocationSynchronizer));
        let events = Arc::new(Mutex::new(Vec::new()));

        let from_events = Arc::clone(&events);
        let _from = navigation
            .on_navigated_from(move |data| {
                from_events.lock().unwrap().push((
                    "from",
                    data.direction,
                    data.from.as_ref().map(|value| value.route.location.clone()),
                    data.to.route.location.clone(),
                    data.to.route.passed_parameters.clone(),
                ));
            })
            .unwrap();
        let to_events = Arc::clone(&events);
        let _to = navigation
            .on_navigated_to(move |data| {
                to_events.lock().unwrap().push((
                    "to",
                    data.direction,
                    data.from.as_ref().map(|value| value.route.location.clone()),
                    data.to.route.location.clone(),
                    data.to.route.passed_parameters.clone(),
                ));
            })
            .unwrap();

        navigation
            .navigate(NavigationRequest {
                region: "root".to_string(),
                args: NavigationArgs {
                    parameters: Some(json!({"source": "initial"})),
                    ..NavigationArgs::new("/a")
                },
            })
            .unwrap();
        navigation.navigate_to("/b").unwrap();
        navigation.back().unwrap();
        navigation.forward().unwrap();
        navigation.refresh().unwrap();

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
            navigation.navigate_to("/b").unwrap().status,
            RoutingStatus::SkippedSameRoute
        );
        assert_ne!(
            navigation.navigate_to("/missing").unwrap().status,
            RoutingStatus::Success
        );
        assert_ne!(
            navigation
                .navigate_to("https://external.example/")
                .unwrap()
                .status,
            RoutingStatus::Success
        );
        assert_eq!(events.lock().unwrap().len(), before_unsuccessful);
    }

    #[test]
    fn lifecycle_callbacks_release_state_and_ignore_no_history() {
        let mut routes = RouteMap::default();
        routes.route("/a").finish();
        routes.route("/b").finish();
        let navigation = NavigationService::new(routes)
            .with_location_synchronizer(Arc::new(NoopNavigationLocationSynchronizer));
        let order = Arc::new(Mutex::new(Vec::new()));

        let from_order = Arc::clone(&order);
        let navigation_from = navigation.clone();
        let _from = navigation
            .on_navigated_from(move |_| {
                from_order.lock().unwrap().push("from");
                assert_eq!(
                    navigation_from.current().unwrap().unwrap().route.location,
                    "/b"
                );
            })
            .unwrap();
        let to_order = Arc::clone(&order);
        let _to = navigation
            .on_navigated_to(move |_| to_order.lock().unwrap().push("to"))
            .unwrap();
        let route_order = Arc::clone(&order);
        let _route = navigation
            .subscribe_route_changed(move |_| route_order.lock().unwrap().push("route"))
            .unwrap();
        let location_order = Arc::clone(&order);
        let _location = navigation
            .subscribe_location_changed(move |_| location_order.lock().unwrap().push("location"))
            .unwrap();

        navigation.navigate_to("/a").unwrap();
        order.lock().unwrap().clear();
        navigation.navigate_to("/b").unwrap();
        assert_eq!(
            order.lock().unwrap().as_slice(),
            ["from", "to", "route", "location"]
        );

        let mut no_history_routes = RouteMap::default();
        no_history_routes.route("/").finish();
        let no_history = NavigationService::new(no_history_routes)
            .with_location_synchronizer(Arc::new(NoopNavigationLocationSynchronizer));
        let callback_count = Arc::new(Mutex::new(0usize));
        let observed = Arc::clone(&callback_count);
        let _to = no_history
            .on_navigated_to(move |_| *observed.lock().unwrap() += 1)
            .unwrap();

        assert_eq!(
            no_history.back().unwrap().status,
            RoutingStatus::SkippedNoHistory
        );
        assert_eq!(
            no_history.refresh().unwrap().status,
            RoutingStatus::SkippedNoHistory
        );
        assert_eq!(*callback_count.lock().unwrap(), 0);
    }
}

mod service {
    use rayx_runtime::services::navigation::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn test_navigation_service() -> NavigationService {
        let mut routes = RouteMap::default();
        routes.route("/a").finish();
        routes.route("/b").finish();
        NavigationService::new(routes)
    }

    #[test]
    fn route_params_match_braced_segments() {
        let mut routes = RouteMap::default();
        routes.route("/sports/{league}").finish();
        let matched = routes.process_route("/sports/nba", None).unwrap();
        assert_eq!(matched.status, RoutingStatus::Success);
        assert_eq!(matched.route.params.get("league"), Some("nba"));
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
            fn synchronize(&self, route: &Route, mode: NavigationHistoryMode) -> Result<()> {
                self.calls
                    .lock()
                    .unwrap()
                    .push((route.location.clone(), mode));
                Ok(())
            }
        }

        let mut routes = RouteMap::default();
        routes.route("/sports/{league}").finish();
        let recorder = Arc::new(Recorder::default());
        let navigation =
            NavigationService::new(routes).with_location_synchronizer(recorder.clone());
        let result = navigation.navigate(NavigationRequest {
            region: "root".to_string(),
            args: NavigationArgs::new("/sports/nba"),
        })?;
        assert_eq!(result.route.params.get("league"), Some("nba"));
        assert_eq!(
            recorder.calls.lock().unwrap().as_slice(),
            &[("/sports/nba".to_string(), NavigationHistoryMode::Push)]
        );
        Ok(())
    }

    #[test]
    fn named_regions_have_independent_journals_and_root_is_protected() {
        let mut routes = RouteMap::default();
        routes.route("/home").finish();
        routes.route("/tool").finish();
        let navigation = NavigationService::new(routes);

        navigation.navigate_to("/home").unwrap();
        navigation
            .navigate_in_region("sidebar", NavigationArgs::new("/tool"))
            .unwrap();

        assert_eq!(
            navigation
                .current_in_region("root")
                .unwrap()
                .unwrap()
                .route
                .location,
            "/home"
        );
        assert_eq!(
            navigation
                .current_in_region("sidebar")
                .unwrap()
                .unwrap()
                .route
                .location,
            "/tool"
        );
        assert!(!navigation.remove_region("root").unwrap());
        assert!(navigation.remove_region("sidebar").unwrap());
    }

    #[test]
    fn failed_routes_can_use_configured_not_found_fallback() {
        let options = NavigationOptions {
            not_found_route: Some("/not-found".to_string()),
            routes: vec![RouteDefinition::new("/not-found")],
            ..NavigationOptions::default()
        };
        let navigation = NavigationService::with_options(options).unwrap();

        let result = navigation.navigate_to("/missing").unwrap();
        assert_eq!(result.status, RoutingStatus::Success);
        assert_eq!(result.route.location, "/not-found");
    }

    #[test]
    fn navigation_lifecycle_subscribers_publish_complete_ordered_data() {
        let mut routes = RouteMap::default();
        routes.route("/a").finish();
        routes.route("/b").finish();
        let navigation = NavigationService::new(routes);
        let events = Arc::new(Mutex::new(Vec::new()));

        let from_events = Arc::clone(&events);
        let _from = navigation
            .on_navigated_from(move |data| {
                from_events.lock().unwrap().push((
                    "from",
                    data.direction,
                    data.from.as_ref().map(|value| value.route.location.clone()),
                    data.to.route.location.clone(),
                    data.to.route.passed_parameters.clone(),
                ));
            })
            .unwrap();
        let to_events = Arc::clone(&events);
        let _to = navigation
            .on_navigated_to(move |data| {
                to_events.lock().unwrap().push((
                    "to",
                    data.direction,
                    data.from.as_ref().map(|value| value.route.location.clone()),
                    data.to.route.location.clone(),
                    data.to.route.passed_parameters.clone(),
                ));
            })
            .unwrap();

        navigation
            .navigate(NavigationRequest {
                region: "root".to_string(),
                args: NavigationArgs {
                    parameters: Some(json!({"source": "initial"})),
                    ..NavigationArgs::new("/a")
                },
            })
            .unwrap();
        navigation.navigate_to("/b").unwrap();
        navigation.back().unwrap();
        navigation.forward().unwrap();
        navigation.refresh().unwrap();

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
    }

    #[test]
    fn navigation_lifecycle_subscribers_dispatch_before_legacy_events_and_reenter() {
        let navigation = test_navigation_service();
        let order = Arc::new(Mutex::new(Vec::new()));

        let from_order = Arc::clone(&order);
        let navigation_from = navigation.clone();
        let _from = navigation
            .on_navigated_from(move |_| {
                from_order.lock().unwrap().push("from");
                assert_eq!(
                    navigation_from.current().unwrap().unwrap().route.location,
                    "/b"
                );
            })
            .unwrap();
        let to_order = Arc::clone(&order);
        let _to = navigation
            .on_navigated_to(move |_| to_order.lock().unwrap().push("to"))
            .unwrap();
        let route_order = Arc::clone(&order);
        let _route = navigation
            .subscribe_route_changed(move |_| route_order.lock().unwrap().push("route"))
            .unwrap();
        let location_order = Arc::clone(&order);
        let _location = navigation
            .subscribe_location_changed(move |_| location_order.lock().unwrap().push("location"))
            .unwrap();

        navigation.navigate_to("/a").unwrap();
        order.lock().unwrap().clear();
        navigation.navigate_to("/b").unwrap();

        assert_eq!(
            order.lock().unwrap().as_slice(),
            ["from", "to", "route", "location"]
        );
    }

    #[test]
    fn navigation_lifecycle_subscribers_ignore_unsuccessful_outcomes() {
        let navigation = test_navigation_service();
        let events = Arc::new(Mutex::new(0usize));
        let observed = Arc::clone(&events);
        let _subscription = navigation
            .on_navigated_to(move |_| *observed.lock().unwrap() += 1)
            .unwrap();

        navigation.navigate_to("/a").unwrap();
        assert_eq!(*events.lock().unwrap(), 1);
        navigation.navigate_to("/a").unwrap();
        navigation.navigate_to("/missing").unwrap();
        navigation.navigate_to("https://external.example/").unwrap();
        navigation.back().unwrap();
        navigation.back().unwrap();

        assert_eq!(*events.lock().unwrap(), 1);
    }
}

mod registration {
    use rayx_core::RayXAppBuilder;
    use rayx_runtime::services::navigation::{
        NavigationAppBuilderExt, NavigationOptions, NavigationService, RouteDefinition,
    };
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
        let navigation = services.required_service::<NavigationService>()?;

        let observed = observed.lock().unwrap();
        assert_eq!(observed[0].default_route, "/layered");
        assert_eq!(observed[0].max_journal_entries, 72);
        assert_eq!(options.value(), navigation.options());
        assert_eq!(navigation.options().max_journal_entries, 10);
        assert_eq!(navigation.options().routes.len(), 1);
        assert!(services.has_startup_role::<NavigationService>());
        Ok(())
    }

    #[test]
    fn a_registered_navigation_service_wins() -> Result<(), Box<dyn std::error::Error>> {
        let custom = || {
            NavigationService::with_options(NavigationOptions {
                default_route: "/custom".to_string(),
                ..NavigationOptions::default()
            })
            .unwrap()
        };
        let before = RayXAppBuilder::new()
            .with_navigation_service(custom())
            .add_navigation(|options| options.default_route = "/default".to_string())
            .build()?;
        let after = RayXAppBuilder::new()
            .add_navigation(|options| options.default_route = "/default".to_string())
            .with_navigation_service(custom())
            .build()?;
        for services in [before, after] {
            let navigation = services.required_service::<NavigationService>()?;
            assert_eq!(navigation.options().default_route, "/custom");
        }
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
