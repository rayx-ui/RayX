//! The navigation's location: the URL region's route behind the base path with the other
//! regions in its query, shown through the synchronizer after every navigation in any region,
//! and taken in from the platform (back, forward, a typed address, the startup deep link).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_core::di::StartupInitializationContext;
use rayx_core::{RayXAppBuilder, ServiceProvider};
use rayx_runtime::services::navigation::{
    NavigationAppBuilderExt, NavigationArgs, NavigationDirection, NavigationHistoryMode,
    NavigationLocationSynchronizer, NavigationOptions, NavigationService, Result, RouteDefinition,
    RoutingStatus,
};

/// The platform's side: what it showed, and the location it starts at.
#[derive(Clone, Default)]
struct Platform {
    shown: Arc<Mutex<Vec<(String, NavigationHistoryMode)>>>,
    start: Option<String>,
}

impl NavigationLocationSynchronizer for Platform {
    fn location(&self) -> Option<String> {
        self.start.clone()
    }

    fn synchronize(&self, location: &str, mode: NavigationHistoryMode) -> Result<()> {
        self.shown
            .lock()
            .unwrap()
            .push((location.to_string(), mode));
        Ok(())
    }
}

impl Platform {
    fn last(&self) -> Option<(String, NavigationHistoryMode)> {
        self.shown.lock().unwrap().last().cloned()
    }

    fn count(&self) -> usize {
        self.shown.lock().unwrap().len()
    }
}

fn routes(options: &mut NavigationOptions) {
    options.add_route(RouteDefinition::new("/"));
    options.add_route(RouteDefinition::new("/teams/{id}"));
    options.add_route(RouteDefinition::new("/bets"));
    options.add_route(RouteDefinition::new("/bets/{id}"));
}

/// The app started with `platform` as its synchronizer (from DI) and `configure`d options.
fn start(
    platform: &Platform,
    configure: impl Fn(&mut NavigationOptions) + Send + Sync + 'static,
) -> (ServiceProvider, Arc<NavigationService>) {
    let services = RayXAppBuilder::new()
        .with_navigation_location_synchronizer(platform.clone())
        .add_navigation(move |options| {
            routes(options);
            configure(options);
        })
        .build()
        .unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let navigation = services.required_service::<NavigationService>().unwrap();
    (services, navigation)
}

fn in_panel(navigation: &NavigationService, uri: &str) {
    let result = navigation
        .navigate_in_region("panel", NavigationArgs::new(uri))
        .unwrap();
    assert_eq!(result.status, RoutingStatus::Success, "{uri}");
}

#[test]
fn the_location_is_the_url_region_s_route_with_the_others_in_its_query() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    navigation.navigate_to("/teams/12").unwrap();
    assert_eq!(platform.last().unwrap().0, "/teams/12");
    in_panel(&navigation, "/bets/3");
    assert_eq!(
        navigation.location().unwrap(),
        "/teams/12?panel=%2Fbets%2F3"
    );
    // A navigation in another region shows the whole location, as a new entry.
    assert_eq!(
        platform.last().unwrap(),
        (
            "/teams/12?panel=%2Fbets%2F3".to_string(),
            NavigationHistoryMode::Push
        )
    );
}

#[test]
fn back_forward_and_refresh_in_another_region_show_the_whole_location() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    navigation.navigate_to("/teams/12").unwrap();
    in_panel(&navigation, "/bets");
    in_panel(&navigation, "/bets/3");
    navigation.back_in_region("panel").unwrap();
    // Not the panel's own route, which the URL showed before.
    assert_eq!(
        platform.last().unwrap(),
        (
            "/teams/12?panel=%2Fbets".to_string(),
            NavigationHistoryMode::Replace
        )
    );
    navigation.forward_in_region("panel").unwrap();
    assert_eq!(platform.last().unwrap().0, "/teams/12?panel=%2Fbets%2F3");
    navigation.refresh_in_region("panel").unwrap();
    assert_eq!(platform.last().unwrap().0, "/teams/12?panel=%2Fbets%2F3");
}

#[test]
fn a_base_path_and_another_url_region_shape_the_location() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |options| {
        options.base_path = "/app/".to_string();
        options.url_region = Some("main".to_string());
    });
    navigation
        .navigate_in_region("main", NavigationArgs::new("/teams/4?tab=odds"))
        .unwrap();
    assert_eq!(navigation.url_region(), "main");
    // Startup took the URL region to the default route; the root region has none to carry.
    assert_eq!(navigation.location().unwrap(), "/app/teams/4?tab=odds");
    navigation
        .navigate_in_region("root", NavigationArgs::new("/bets"))
        .unwrap();
    assert_eq!(
        navigation.location().unwrap(),
        "/app/teams/4?tab=odds&root=%2Fbets"
    );
    // A location outside the base path is refused.
    assert!(navigation.apply_location("/elsewhere/teams/4").is_err());
    let results = navigation.apply_location("/app/teams/5").unwrap();
    assert_eq!(results[0].region, "main");
    assert_eq!(results[0].route.path, "/teams/5");
}

#[test]
fn the_platform_s_back_and_forward_move_the_journal_without_showing_it_again() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    navigation.navigate_to("/teams/1").unwrap();
    navigation.navigate_to("/teams/2").unwrap();
    let shown = platform.count();

    // The browser went back: the journal's entry before, not a new one.
    let results = navigation.apply_location("/teams/1").unwrap();
    assert_eq!(results[0].direction, NavigationDirection::Back);
    let journal = navigation.journal("root").unwrap().unwrap();
    assert_eq!(journal.count(), 3, "/, /teams/1, /teams/2");
    assert_eq!(journal.current_index(), Some(1));
    // And forward again.
    let results = navigation.apply_location("/teams/2").unwrap();
    assert_eq!(results[0].direction, NavigationDirection::Forward);
    assert_eq!(
        navigation.journal("root").unwrap().unwrap().current_index(),
        Some(2)
    );
    // A typed address is a new entry; the same location again changes nothing.
    let results = navigation.apply_location("/teams/9").unwrap();
    assert_eq!(results[0].direction, NavigationDirection::New);
    assert_eq!(navigation.journal("root").unwrap().unwrap().count(), 4);
    let results = navigation.apply_location("/teams/9").unwrap();
    assert_eq!(results[0].status, RoutingStatus::SkippedSameRoute);
    assert_eq!(platform.count(), shown, "the platform is there already");
}

#[test]
fn the_location_opens_and_closes_query_regions() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |options| {
        options.query_regions = vec!["panel".to_string()];
    });
    let closed = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&closed);
    let _subscription = navigation
        .subscribe_region_closed(move |region| heard.lock().unwrap().push(region.to_string()))
        .unwrap();

    // A declared query region is taken from the query before it exists; other keys stay the
    // URL region's.
    navigation
        .apply_location("/teams/3?tab=odds&panel=%2Fbets%2F2")
        .unwrap();
    assert_eq!(
        navigation.current().unwrap().unwrap().route.location,
        "/teams/3?tab=odds"
    );
    assert_eq!(
        navigation
            .current_in_region("panel")
            .unwrap()
            .unwrap()
            .route
            .location,
        "/bets/2"
    );
    // A location without it closes it.
    navigation.apply_location("/teams/3?tab=odds").unwrap();
    assert!(navigation.current_in_region("panel").unwrap().is_none());
    assert_eq!(*closed.lock().unwrap(), ["panel"]);
    assert_eq!(navigation.location().unwrap(), "/teams/3?tab=odds");

    // The app closes one itself: the location drops it, as a new entry.
    in_panel(&navigation, "/bets");
    assert!(navigation.close_region("panel").unwrap());
    assert!(!navigation.close_region("panel").unwrap(), "closed already");
    assert!(
        !navigation.close_region("root").unwrap(),
        "the URL region stays"
    );
    assert_eq!(platform.last().unwrap().0, "/teams/3?tab=odds");
}

#[test]
fn a_journal_entry_is_reached_by_its_index() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    navigation.navigate_to("/teams/1").unwrap();
    navigation.navigate_to("/teams/2").unwrap();
    let result = navigation.go_to_entry("root", 0).unwrap();
    assert_eq!(result.direction, NavigationDirection::Back);
    assert_eq!(result.route.path, "/");
    assert_eq!(
        platform.last().unwrap(),
        ("/".to_string(), NavigationHistoryMode::Replace)
    );
    let result = navigation.go_to_entry("root", 2).unwrap();
    assert_eq!(result.direction, NavigationDirection::Forward);
    assert!(navigation.go_to_entry("root", 7).is_err());
    assert!(navigation.go_to_entry("nowhere", 0).is_err());
}

#[test]
fn startup_takes_the_platform_s_location_as_its_deep_link() {
    let platform = Platform {
        start: Some("/teams/7?panel=%2Fbets".to_string()),
        ..Platform::default()
    };
    let (_services, navigation) = start(&platform, |options| {
        options.query_regions = vec!["panel".to_string()];
    });
    assert_eq!(
        navigation.current().unwrap().unwrap().route.path,
        "/teams/7"
    );
    assert_eq!(
        navigation
            .current_in_region("panel")
            .unwrap()
            .unwrap()
            .route
            .path,
        "/bets"
    );
    assert_eq!(navigation.journal("root").unwrap().unwrap().count(), 1);

    // A deep link the routes do not know starts at the default route.
    let lost = Platform {
        start: Some("/nowhere".to_string()),
        ..Platform::default()
    };
    let (_services, navigation) = start(&lost, |_| {});
    assert_eq!(navigation.current().unwrap().unwrap().route.path, "/");
}
