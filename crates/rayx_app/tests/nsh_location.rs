//! The navigation's location on NSH, headless, through the host channel an embedding app has: it
//! starts at NSH's location (a deep link), every navigation reaches NSH's location (a push as a
//! new history entry, the other modes in place), the navigations of one turn share an entry, and
//! the platform's moves (back, forward, a typed address) take the navigation along.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::location::{HistoryBehavior, Navigation};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, HostChannel, LocalEngine, MountSink};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::navigation::{
    NavigationArgs, NavigationHistoryMode, NavigationService, RouteDefinition,
};
use rayx_app::{
    AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, navigation_view, set_app,
};

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// The tests share the app (one composition root per process): one at a time.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_navigation(|options| {
        options.add_route(RouteDefinition::new("/"));
        options.add_route(RouteDefinition::new("/projects/{id}"));
        options.add_route(RouteDefinition::new("/settings"));
        options.query_regions.push("panel".into());
    })
}

/// The root region's route and the `panel` region's.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOT.with_borrow_mut(|root| *root = Some(app_services().unwrap()));
            let page = navigation_view("root", |route| {
                text(format!(
                    "at {}",
                    route.map_or("nowhere".to_string(), |route| route.path.clone())
                ))
            })
            .unwrap();
            let panel = navigation_view("panel", |route| {
                text(format!(
                    "panel {}",
                    route.map_or("closed".to_string(), |route| route.path.clone())
                ))
            })
            .unwrap();
            view().child(page).child(panel).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

/// An app on a headless engine, with the host channel an embedding app would hold and the
/// navigations it heard.
struct Running {
    engine: LocalEngine<ApproxTextEngine>,
    rx: Receiver<MountBatch>,
    host: HostChannel,
    heard: Arc<Mutex<Vec<(String, HistoryBehavior)>>>,
    /// The texts its first screens showed.
    started: Vec<String>,
}

impl Running {
    /// Starts the app at `location` (a deep link) and waits for it to be ready.
    fn start(location: Option<&str>) -> Running {
        set_app(build_app);
        let host = HostChannel::new();
        if let Some(location) = location {
            host.set_location(location).unwrap();
        }
        let heard = Arc::new(Mutex::new(Vec::new()));
        let navigations = Arc::clone(&heard);
        host.handle_navigate(move |navigation: Navigation| {
            navigations
                .lock()
                .unwrap()
                .push((navigation.location.as_str().to_owned(), navigation.history));
        });
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = LocalEngine::start(
            EngineConfig {
                host: host.clone(),
                ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
            },
            ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap();
        let mut running = Running {
            engine,
            rx,
            host,
            heard,
            started: Vec::new(),
        };
        running.started =
            running.wait_for(|shown| shown.iter().any(|text| text.starts_with("panel ")));
        running
    }

    fn navigation(&self) -> Arc<NavigationService> {
        ROOT.with_borrow(|root| root.clone())
            .expect("the app started")
            .required_service::<NavigationService>()
            .unwrap()
    }

    /// Pumps the engine until `done` holds for the texts it mounted (or 10 s); the texts.
    fn wait_for(&mut self, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let mut shown = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            self.engine.pump();
            while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(5)) {
                for item in &batch.items {
                    if let MountItem::Text { text, .. } = item {
                        shown.push(text.text.clone());
                    }
                }
            }
            if done(&shown) {
                break;
            }
        }
        shown
    }

    /// Pumps until the host's location is `location`.
    fn wait_for_location(&mut self, location: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while self.host.location().as_str() != location && std::time::Instant::now() < deadline {
            self.engine.pump();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(self.host.location().as_str(), location);
    }

    fn heard(&self) -> Vec<(String, HistoryBehavior)> {
        self.heard.lock().unwrap().clone()
    }

    fn root_index(&self) -> Option<usize> {
        self.navigation()
            .journal("root")
            .unwrap()
            .unwrap()
            .current_index()
    }
}

fn shows(shown: &[String], text: &str) -> bool {
    shown.iter().any(|shown| shown == text)
}

#[test]
fn a_deep_link_starts_the_navigation_there() {
    let _serial = serial();
    let app = Running::start(Some("/projects/1?panel=%2Fsettings"));
    let shown = &app.started;
    assert!(shows(shown, "at /projects/1"), "shown: {shown:?}");
    assert!(shows(shown, "panel /settings"), "shown: {shown:?}");
    assert_eq!(
        app.navigation().location().unwrap(),
        "/projects/1?panel=%2Fsettings"
    );
    // The platform is there already: nothing is sent back.
    assert_eq!(app.heard(), []);
}

#[test]
fn navigations_reach_nsh_s_location() {
    let _serial = serial();
    let mut app = Running::start(None);
    let navigation = app.navigation();

    navigation.navigate_to("/projects/1").unwrap();
    app.wait_for_location("/projects/1");
    navigation
        .navigate_in_region("panel", NavigationArgs::new("/settings"))
        .unwrap();
    app.wait_for_location("/projects/1?panel=%2Fsettings");
    navigation
        .navigate_in_region(
            "root",
            NavigationArgs {
                history_mode: NavigationHistoryMode::Replace,
                ..NavigationArgs::new("/projects/2")
            },
        )
        .unwrap();
    app.wait_for_location("/projects/2?panel=%2Fsettings");
    // Closing a region is a navigation of the default mode, a push: back opens it again.
    navigation.close_region("panel").unwrap();
    app.wait_for_location("/projects/2");

    assert_eq!(
        app.heard(),
        [
            ("/projects/1".to_owned(), HistoryBehavior::Push),
            (
                "/projects/1?panel=%2Fsettings".to_owned(),
                HistoryBehavior::Push
            ),
            (
                "/projects/2?panel=%2Fsettings".to_owned(),
                HistoryBehavior::Replace
            ),
            ("/projects/2".to_owned(), HistoryBehavior::Push),
        ]
    );
}

#[test]
fn the_navigations_of_one_turn_share_a_history_entry() {
    let _serial = serial();
    let mut app = Running::start(None);
    let navigation = app.navigation();
    navigation.navigate_to("/projects/1").unwrap();
    navigation.navigate_to("/settings").unwrap();
    navigation
        .navigate_in_region("panel", NavigationArgs::new("/projects/2"))
        .unwrap();
    app.wait_for_location("/settings?panel=%2Fprojects%2F2");
    let shown = app.wait_for(|shown| shows(shown, "panel /projects/2"));
    assert_eq!(
        app.heard(),
        [(
            "/settings?panel=%2Fprojects%2F2".to_owned(),
            HistoryBehavior::Push
        )]
    );
    // NSH's location did not go back through the earlier ones.
    assert!(!shows(&shown, "panel closed"), "shown: {shown:?}");
    assert_eq!(app.root_index(), Some(2));
}

#[test]
fn the_platform_s_moves_take_the_navigation_along() {
    let _serial = serial();
    let mut app = Running::start(None);
    let navigation = app.navigation();
    navigation.navigate_to("/projects/1").unwrap();
    app.wait_for_location("/projects/1");
    navigation.navigate_to("/settings").unwrap();
    app.wait_for_location("/settings");
    let sent = app.heard().len();

    // Back: the journal goes back to the entry that has the location.
    app.host.set_location("/projects/1").unwrap();
    let shown = app.wait_for(|shown| shows(shown, "at /projects/1"));
    assert!(shows(&shown, "at /projects/1"), "shown: {shown:?}");
    assert_eq!(app.root_index(), Some(1));

    // Forward, with a panel open in the query.
    app.host
        .set_location("/settings?panel=%2Fprojects%2F3")
        .unwrap();
    let shown =
        app.wait_for(|shown| shows(shown, "at /settings") && shows(shown, "panel /projects/3"));
    assert!(shows(&shown, "panel /projects/3"), "shown: {shown:?}");
    assert_eq!(app.root_index(), Some(2));

    // A location without the panel closes it.
    app.host.set_location("/settings").unwrap();
    let shown = app.wait_for(|shown| shows(shown, "panel closed"));
    assert!(shows(&shown, "panel closed"), "shown: {shown:?}");

    // The platform's moves are not the app's: nothing went back to the host.
    assert_eq!(app.heard().len(), sent);
    assert_eq!(navigation.location().unwrap(), "/settings");
}
