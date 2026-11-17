//! The navigation's location: the URL region's route behind the base path with the other
//! regions in its query, shown through the synchronizer after every navigation in any region,
//! and taken in from the platform (back, forward, a typed address, the startup deep link).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::navigation::{
    NavigationArgs, NavigationDirection, NavigationHistoryMode, NavigationService, Result,
    RoutingStatus,
};
use rayx_app::{RayXAppBuilder, ServiceProvider};
use rayx_services_navigation::testing::{NavigationLocationSynchronizer, add_navigation_with};
use rayx_services_navigation::{NavigationOptions, RouteDefinition};

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
    options.add_route(RouteDefinition::new("/projects/{id}"));
    options.add_route(RouteDefinition::new("/settings"));
    options.add_route(RouteDefinition::new("/settings/{id}"));
}

/// The app started with `platform` as its synchronizer and `configure`d options.
fn start(
    platform: &Platform,
    configure: impl Fn(&mut NavigationOptions) + Send + Sync + 'static,
) -> (ServiceProvider, Arc<dyn NavigationService>) {
    let services = add_navigation_with(
        RayXAppBuilder::new(),
        move |options| {
            routes(options);
            configure(options);
        },
        Arc::new(platform.clone()),
    )
    .build()
    .unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let navigation = services
        .required_service::<dyn NavigationService>()
        .unwrap();
    (services, navigation)
}

fn in_panel(navigation: &Arc<dyn NavigationService>, uri: &str) {
    let result =
        block_on(navigation.navigate_in_region("panel", NavigationArgs::new(uri))).unwrap();
    assert_eq!(result.status, RoutingStatus::Success, "{uri}");
}

#[test]
fn the_location_is_the_url_region_s_route_with_the_others_in_its_query() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    block_on(navigation.navigate_to("/projects/12")).unwrap();
    assert_eq!(platform.last().unwrap().0, "/projects/12");
    in_panel(&navigation, "/settings/3");
    assert_eq!(navigation.location(), "/projects/12?panel=%2Fsettings%2F3");
    // A navigation in another region shows the whole location, as a new entry.
    assert_eq!(
        platform.last().unwrap(),
        (
            "/projects/12?panel=%2Fsettings%2F3".to_string(),
            NavigationHistoryMode::Push
        )
    );
}

#[test]
fn back_forward_and_refresh_in_another_region_show_the_whole_location() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    block_on(navigation.navigate_to("/projects/12")).unwrap();
    in_panel(&navigation, "/settings");
    in_panel(&navigation, "/settings/3");
    block_on(navigation.back("panel".into())).unwrap();
    // Not the panel's own route, which the URL showed before.
    assert_eq!(
        platform.last().unwrap(),
        (
            "/projects/12?panel=%2Fsettings".to_string(),
            NavigationHistoryMode::Replace
        )
    );
    block_on(navigation.forward("panel".into())).unwrap();
    assert_eq!(
        platform.last().unwrap().0,
        "/projects/12?panel=%2Fsettings%2F3"
    );
    block_on(navigation.refresh("panel".into())).unwrap();
    assert_eq!(
        platform.last().unwrap().0,
        "/projects/12?panel=%2Fsettings%2F3"
    );
}

#[test]
fn a_base_path_and_another_url_region_shape_the_location() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |options| {
        options.base_path = "/app/".to_string();
        options.url_region = Some("main".to_string());
    });
    block_on(navigation.navigate_in_region("main", NavigationArgs::new("/projects/4?tab=files")))
        .unwrap();
    assert_eq!(navigation.url_region(), "main");
    // Startup took the URL region to the default route; the root region has none to carry.
    assert_eq!(navigation.location(), "/app/projects/4?tab=files");
    block_on(navigation.navigate_in_region("root", NavigationArgs::new("/settings"))).unwrap();
    assert_eq!(
        navigation.location(),
        "/app/projects/4?tab=files&root=%2Fsettings"
    );
    // A location outside the base path is refused.
    assert!(block_on(navigation.apply_location("/elsewhere/projects/4".into())).is_err());
    let results = block_on(navigation.apply_location("/app/projects/5".into())).unwrap();
    assert_eq!(results[0].region, "main");
    assert_eq!(results[0].route.path, "/projects/5");
}

#[test]
fn the_platform_s_back_and_forward_move_the_journal_without_showing_it_again() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    block_on(navigation.navigate_to("/projects/1")).unwrap();
    block_on(navigation.navigate_to("/projects/2")).unwrap();
    let shown = platform.count();

    // The browser went back: the journal's entry before, not a new one.
    let results = block_on(navigation.apply_location("/projects/1".into())).unwrap();
    assert_eq!(results[0].direction, NavigationDirection::Back);
    let journal = navigation.journal("root").unwrap();
    assert_eq!(journal.count(), 3, "/, /projects/1, /projects/2");
    assert_eq!(journal.current_index(), Some(1));
    // And forward again.
    let results = block_on(navigation.apply_location("/projects/2".into())).unwrap();
    assert_eq!(results[0].direction, NavigationDirection::Forward);
    assert_eq!(navigation.journal("root").unwrap().current_index(), Some(2));
    // A typed address is a new entry; the same location again changes nothing.
    let results = block_on(navigation.apply_location("/projects/9".into())).unwrap();
    assert_eq!(results[0].direction, NavigationDirection::New);
    assert_eq!(navigation.journal("root").unwrap().count(), 4);
    let results = block_on(navigation.apply_location("/projects/9".into())).unwrap();
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
    let _subscription =
        navigation.on_region_closed(move |region| heard.lock().unwrap().push(region.to_string()));

    // A declared query region is taken from the query before it exists; other keys stay the
    // URL region's.
    block_on(navigation.apply_location("/projects/3?tab=files&panel=%2Fsettings%2F2".into()))
        .unwrap();
    assert_eq!(
        navigation.current().unwrap().route.location,
        "/projects/3?tab=files"
    );
    assert_eq!(
        navigation
            .current_in_region("panel")
            .unwrap()
            .route
            .location,
        "/settings/2"
    );
    // A location without it closes it.
    block_on(navigation.apply_location("/projects/3?tab=files".into())).unwrap();
    assert!(navigation.current_in_region("panel").is_none());
    assert_eq!(*closed.lock().unwrap(), ["panel"]);
    assert_eq!(navigation.location(), "/projects/3?tab=files");

    // The app closes one itself: the location drops it, as a new entry.
    in_panel(&navigation, "/settings");
    assert!(block_on(navigation.close_region("panel".into())).unwrap());
    assert!(
        !block_on(navigation.close_region("panel".into())).unwrap(),
        "closed already"
    );
    assert!(
        !block_on(navigation.close_region("root".into())).unwrap(),
        "the URL region stays"
    );
    assert_eq!(platform.last().unwrap().0, "/projects/3?tab=files");
}

#[test]
fn a_journal_entry_is_reached_by_its_index() {
    let platform = Platform::default();
    let (_services, navigation) = start(&platform, |_| {});
    block_on(navigation.navigate_to("/projects/1")).unwrap();
    block_on(navigation.navigate_to("/projects/2")).unwrap();
    let result = block_on(navigation.go_to_entry("root".into(), 0)).unwrap();
    assert_eq!(result.direction, NavigationDirection::Back);
    assert_eq!(result.route.path, "/");
    assert_eq!(
        platform.last().unwrap(),
        ("/".to_string(), NavigationHistoryMode::Replace)
    );
    let result = block_on(navigation.go_to_entry("root".into(), 2)).unwrap();
    assert_eq!(result.direction, NavigationDirection::Forward);
    assert!(block_on(navigation.go_to_entry("root".into(), 7)).is_err());
    assert!(block_on(navigation.go_to_entry("nowhere".into(), 0)).is_err());
}

#[test]
fn startup_takes_the_platform_s_location_as_its_deep_link() {
    let platform = Platform {
        start: Some("/projects/7?panel=%2Fsettings".to_string()),
        ..Platform::default()
    };
    let (_services, navigation) = start(&platform, |options| {
        options.query_regions = vec!["panel".to_string()];
    });
    assert_eq!(navigation.current().unwrap().route.path, "/projects/7");
    assert_eq!(
        navigation.current_in_region("panel").unwrap().route.path,
        "/settings"
    );
    assert_eq!(navigation.journal("root").unwrap().count(), 1);

    // A deep link the routes do not know starts at the default route.
    let lost = Platform {
        start: Some("/nowhere".to_string()),
        ..Platform::default()
    };
    let (_services, navigation) = start(&lost, |_| {});
    assert_eq!(navigation.current().unwrap().route.path, "/");
}
