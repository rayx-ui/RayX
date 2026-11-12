//! The navigation's location on NSH, headless, as the web runs it: it starts at the page's address
//! (a deep link), every navigation reaches NSH's app address (a push as a new history entry, the
//! other modes in place), the navigations of one turn share an entry, and the platform's moves
//! (back, forward, a typed address) take the navigation along. The engine runs as the web OS, the
//! page's address and its moves come as the root's `UrlScheme` events, and the addresses the app
//! sets are the root's `SetAppUrl` and `PushAppUrl` commands.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::platform::{Environment, Os};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, MountBatch, MountItem, PayloadValue,
    ROOT_ID, prelude::*,
};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::location_of_address;
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::navigation::{
    NavigationArgs, NavigationHistoryMode, NavigationService, RouteDefinition,
};
use rayx_app::{
    AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, navigation_view, set_app,
};

/// The page's origin and base.
const PAGE: &str = "https://app.test/";

/// How an address reached the history: a push or in place of the current entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum History {
    Push,
    Replace,
}

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

/// The root's `UrlScheme` event: the page at `url`, or a move of it the app did not make.
fn page_event(url: &str, entries: &[(&'static str, PayloadValue)]) -> EventPayload {
    let mut map = vec![("url", PayloadValue::Str(url.to_owned()))];
    map.extend(entries.iter().cloned());
    EventPayload::Map(map)
}

/// An app on a headless web engine, with the addresses it set.
struct Running {
    engine: LocalEngine<ApproxTextEngine>,
    rx: Receiver<MountBatch>,
    heard: Vec<(String, History)>,
    /// The texts its first screens showed.
    started: Vec<String>,
}

impl Running {
    /// Starts the app on the page at `path` (a deep link) and waits for it to be ready.
    fn start(path: &str) -> Running {
        set_app(build_app);
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = LocalEngine::start(
            EngineConfig {
                environment: Environment {
                    os: Os::Web,
                    ..Environment::default()
                },
                ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
            },
            ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap();
        // The page's address at start, as the web player reports it before the app runs.
        engine.control().dispatch_event(
            ROOT_ID,
            EventKind::UrlScheme,
            page_event(
                &format!("{}{}", PAGE.trim_end_matches('/'), path),
                &[
                    ("page", PayloadValue::Bool(true)),
                    ("quiet", PayloadValue::Bool(true)),
                    ("base", PayloadValue::Str(PAGE.to_owned())),
                ],
            ),
        );
        let mut running = Running {
            engine,
            rx,
            heard: Vec::new(),
            started: Vec::new(),
        };
        running.started =
            running.wait_for(|shown, _| shown.iter().any(|text| text.starts_with("panel ")));
        running
    }

    fn navigation(&self) -> std::sync::Arc<NavigationService> {
        ROOT.with_borrow(|root| root.clone())
            .expect("the app started")
            .required_service::<NavigationService>()
            .unwrap()
    }

    /// Pumps the engine until `done` holds for the texts it mounted and the addresses it set (or
    /// 10 s); the texts.
    fn wait_for(&mut self, done: impl Fn(&[String], &[(String, History)]) -> bool) -> Vec<String> {
        let mut shown = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            self.engine.pump();
            while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(5)) {
                for item in &batch.items {
                    match item {
                        MountItem::Text { text, .. } => shown.push(text.text.clone()),
                        MountItem::Command {
                            command: Command::SetAppUrl(url),
                            ..
                        } => self.heard.push((url.clone(), History::Replace)),
                        MountItem::Command {
                            command: Command::PushAppUrl(url),
                            ..
                        } => self.heard.push((url.clone(), History::Push)),
                        _ => {}
                    }
                }
            }
            if done(&shown, &self.heard) {
                break;
            }
        }
        shown
    }

    /// Pumps until the app last set the address of `path`.
    fn wait_for_address(&mut self, path: &str) {
        let address = address(path);
        self.wait_for(|_, heard| heard.last().is_some_and(|(url, _)| *url == address));
        assert_eq!(
            self.heard.last().map(|(url, _)| url.as_str()),
            Some(address.as_str())
        );
    }

    /// The platform moved the address to `path` (`back`, `forward`, `address`).
    fn moved(&self, path: &str, reason: &str) {
        self.engine.control().dispatch_event(
            ROOT_ID,
            EventKind::UrlScheme,
            page_event(
                &address(path),
                &[
                    ("page", PayloadValue::Bool(true)),
                    ("change", PayloadValue::Str(reason.to_owned())),
                ],
            ),
        );
    }

    fn root_index(&self) -> Option<usize> {
        self.navigation()
            .journal("root")
            .unwrap()
            .unwrap()
            .current_index()
    }
}

/// The page's address of the location `path`.
fn address(path: &str) -> String {
    format!("{}{}", PAGE.trim_end_matches('/'), path)
}

fn shows(shown: &[String], text: &str) -> bool {
    shown.iter().any(|shown| shown == text)
}

#[test]
fn a_deep_link_starts_the_navigation_there() {
    let _serial = serial();
    let app = Running::start("/projects/1?panel=%2Fsettings");
    let shown = &app.started;
    assert!(shows(shown, "at /projects/1"), "shown: {shown:?}");
    assert!(shows(shown, "panel /settings"), "shown: {shown:?}");
    assert_eq!(
        app.navigation().location().unwrap(),
        "/projects/1?panel=%2Fsettings"
    );
    // The page is there already: no address is set, not even the navigation's start.
    assert_eq!(app.heard, []);
}

#[test]
fn navigations_reach_nsh_s_app_address() {
    let _serial = serial();
    let mut app = Running::start("/");
    let navigation = app.navigation();

    navigation.navigate_to("/projects/1").unwrap();
    app.wait_for_address("/projects/1");
    navigation
        .navigate_in_region("panel", NavigationArgs::new("/settings"))
        .unwrap();
    app.wait_for_address("/projects/1?panel=%2Fsettings");
    navigation
        .navigate_in_region(
            "root",
            NavigationArgs {
                history_mode: NavigationHistoryMode::Replace,
                ..NavigationArgs::new("/projects/2")
            },
        )
        .unwrap();
    app.wait_for_address("/projects/2?panel=%2Fsettings");
    // Closing a region is a navigation of the default mode, a push: back opens it again.
    navigation.close_region("panel").unwrap();
    app.wait_for_address("/projects/2");

    assert_eq!(
        app.heard,
        [
            (address("/projects/1"), History::Push),
            (address("/projects/1?panel=%2Fsettings"), History::Push),
            (address("/projects/2?panel=%2Fsettings"), History::Replace),
            (address("/projects/2"), History::Push),
        ]
    );
}

#[test]
fn the_navigations_of_one_turn_share_a_history_entry() {
    let _serial = serial();
    let mut app = Running::start("/");
    let navigation = app.navigation();
    navigation.navigate_to("/projects/1").unwrap();
    navigation.navigate_to("/settings").unwrap();
    navigation
        .navigate_in_region("panel", NavigationArgs::new("/projects/2"))
        .unwrap();
    let at = address("/settings?panel=%2Fprojects%2F2");
    let shown = app.wait_for(|shown, heard| {
        shows(shown, "panel /projects/2") && heard.last().is_some_and(|(url, _)| *url == at)
    });
    assert!(shows(&shown, "panel /projects/2"), "shown: {shown:?}");
    assert_eq!(
        app.heard,
        [(address("/settings?panel=%2Fprojects%2F2"), History::Push)]
    );
    assert!(!shows(&shown, "panel closed"), "shown: {shown:?}");
    assert_eq!(app.root_index(), Some(2));
}

#[test]
fn the_platform_s_moves_take_the_navigation_along() {
    let _serial = serial();
    let mut app = Running::start("/");
    let navigation = app.navigation();
    navigation.navigate_to("/projects/1").unwrap();
    app.wait_for_address("/projects/1");
    navigation.navigate_to("/settings").unwrap();
    app.wait_for_address("/settings");
    let sent = app.heard.len();

    // Back: the journal goes back to the entry that has the location.
    app.moved("/projects/1", "back");
    let shown = app.wait_for(|shown, _| shows(shown, "at /projects/1"));
    assert!(shows(&shown, "at /projects/1"), "shown: {shown:?}");
    assert_eq!(app.root_index(), Some(1));

    // Forward, with a panel open in the query.
    app.moved("/settings?panel=%2Fprojects%2F3", "forward");
    let shown =
        app.wait_for(|shown, _| shows(shown, "at /settings") && shows(shown, "panel /projects/3"));
    assert!(shows(&shown, "panel /projects/3"), "shown: {shown:?}");
    assert_eq!(app.root_index(), Some(2));

    // A typed address without the panel closes it.
    app.moved("/settings", "address");
    let shown = app.wait_for(|shown, _| shows(shown, "panel closed"));
    assert!(shows(&shown, "panel closed"), "shown: {shown:?}");

    // The platform's moves are not the app's: no address went back to the page.
    assert_eq!(app.heard.len(), sent);
    assert_eq!(navigation.location().unwrap(), "/settings");
}

#[test]
fn an_address_is_a_location_by_its_path_or_what_follows_its_scheme() {
    assert_eq!(
        location_of_address("https://example.com/app/projects/1?panel=%2Fsettings#top"),
        "/app/projects/1?panel=%2Fsettings#top"
    );
    assert_eq!(location_of_address("https://example.com"), "/");
    assert_eq!(
        location_of_address("myapp://projects/1?x=1"),
        "/projects/1?x=1"
    );
    assert_eq!(location_of_address("myapp:///projects/1"), "/projects/1");
    assert_eq!(location_of_address("/projects/1"), "/projects/1");
}
