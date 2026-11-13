//! Deep links on NSH, headless, on every kind of platform: the link that launched a native app,
//! a link that opens it while it runs, and the web's links into a running app (the browser's
//! forward to a page the app no longer has, an earlier load's entry) take the navigation there,
//! and one `base_path` (the web's `web.base`) serves the native links too.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::platform::{Environment, Os};
use nsh_core::{
    ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID,
    prelude::*,
};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::navigation::{NavigationService, RouteDefinition};
use rayx_app::{
    AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, navigation_view, set_app,
};

/// The web page's origin, and the app's base on it (`nsh.json`'s `web.base`).
const ORIGIN: &str = "https://app.test";
const BASE: &str = "/app";

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

/// One navigation for every platform: its locations under the web's base.
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_navigation(|options| {
        options.base_path = BASE.into();
        options.add_route(RouteDefinition::new("/"));
        options.add_route(RouteDefinition::new("/projects/{id}"));
        options.add_route(RouteDefinition::new("/settings"));
        options.query_regions.push("panel".into());
    })
}

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

struct Running {
    engine: LocalEngine<ApproxTextEngine>,
    rx: Receiver<MountBatch>,
    shown: Vec<String>,
}

impl Running {
    /// Starts the app on `os` (claiming `myapp:`), with `before` sent before it runs (what the
    /// platform reports at launch), and waits for it to be ready.
    fn start(os: Os, before: impl FnOnce(&LocalEngine<ApproxTextEngine>)) -> Running {
        set_app(build_app);
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = LocalEngine::start(
            EngineConfig {
                environment: Environment {
                    os,
                    ..Environment::default()
                },
                ..EngineConfig::new(App::Rust(
                    nsh_core::element::Root::of::<Root>().url_schemes(["myapp"]),
                ))
            },
            ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap();
        before(&engine);
        let mut running = Running {
            engine,
            rx,
            shown: Vec::new(),
        };
        running.wait_for(|shown| shown.iter().any(|text| text.starts_with("panel ")));
        running
    }

    fn navigation(&self) -> std::sync::Arc<NavigationService> {
        ROOT.with_borrow(|root| root.clone())
            .expect("the app started")
            .required_service::<NavigationService>()
            .unwrap()
    }

    /// Pumps the engine until `done` holds for the texts it mounted since the last wait (or
    /// 10 s).
    fn wait_for(&mut self, done: impl Fn(&[String]) -> bool) {
        self.pump_for(Duration::from_secs(10), done);
    }

    /// [`Self::wait_for`], for at most `limit`.
    fn pump_for(&mut self, limit: Duration, done: impl Fn(&[String]) -> bool) {
        self.shown.clear();
        let deadline = std::time::Instant::now() + limit;
        while std::time::Instant::now() < deadline {
            self.engine.pump();
            while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(5)) {
                for item in &batch.items {
                    if let MountItem::Text { text, .. } = item {
                        self.shown.push(text.text.clone());
                    }
                }
            }
            if done(&self.shown) {
                break;
            }
        }
    }

    fn shows(&self, text: &str) -> bool {
        self.shown.iter().any(|shown| shown == text)
    }
}

/// The root's `UrlScheme` event for `url` with `entries`.
fn url_event(engine: &LocalEngine<ApproxTextEngine>, url: &str, entries: &[(&'static str, bool)]) {
    let mut map = vec![("url", PayloadValue::Str(url.to_owned()))];
    map.extend(
        entries
            .iter()
            .map(|(key, value)| (*key, PayloadValue::Bool(*value))),
    );
    engine
        .control()
        .dispatch_event(ROOT_ID, EventKind::UrlScheme, EventPayload::Map(map));
}

/// The web page at `path` under the origin, as the web player reports it at start.
fn page_at_start(engine: &LocalEngine<ApproxTextEngine>, path: &str) {
    let mut map = vec![
        ("url", PayloadValue::Str(format!("{ORIGIN}{path}"))),
        ("page", PayloadValue::Bool(true)),
        ("quiet", PayloadValue::Bool(true)),
    ];
    map.push(("base", PayloadValue::Str(format!("{ORIGIN}{BASE}/"))));
    engine
        .control()
        .dispatch_event(ROOT_ID, EventKind::UrlScheme, EventPayload::Map(map));
}

#[test]
fn the_link_that_launched_a_native_app_starts_it_there_under_the_base_path() {
    let _serial = serial();
    for os in [
        Os::Ios,
        Os::Android,
        Os::MacOs,
        Os::Windows,
        Os::Linux,
        Os::Tizen,
    ] {
        let mut app = Running::start(os, |engine| {
            url_event(engine, "myapp://projects/7?panel=%2Fsettings", &[]);
        });
        if !app.shows("at /projects/7") {
            app.wait_for(|shown| shown.iter().any(|text| text == "at /projects/7"));
        }
        assert_eq!(
            app.navigation().location().unwrap(),
            "/app/projects/7?panel=%2Fsettings",
            "{os:?}"
        );
    }
}

#[test]
fn a_link_that_opens_a_running_native_app_takes_the_navigation_there() {
    let _serial = serial();
    for os in [
        Os::Ios,
        Os::Android,
        Os::MacOs,
        Os::Windows,
        Os::Linux,
        Os::Tizen,
    ] {
        let mut app = Running::start(os, |_| {});
        assert_eq!(app.navigation().location().unwrap(), "/app/", "{os:?}");
        url_event(&app.engine, "myapp://projects/9", &[]);
        app.wait_for(|shown| shown.iter().any(|text| text == "at /projects/9"));
        assert!(app.shows("at /projects/9"), "{os:?}: {:?}", app.shown);
        assert_eq!(app.navigation().location().unwrap(), "/app/projects/9");
        // A link of another scheme is not the app's: NSH drops it.
        url_event(&app.engine, "other://projects/1", &[]);
        app.pump_for(Duration::from_millis(200), |_| false);
        assert_eq!(app.navigation().location().unwrap(), "/app/projects/9");
    }
}

#[test]
fn a_link_the_web_hands_a_running_app_takes_the_navigation_there() {
    let _serial = serial();
    let mut app = Running::start(Os::Web, |engine| page_at_start(engine, "/app/"));
    assert_eq!(app.navigation().location().unwrap(), "/app/");
    // The browser's forward to a page the app no longer has (or an earlier load's entry): the
    // web player hands the page's address to the app as a link.
    url_event(
        &app.engine,
        &format!("{ORIGIN}/app/projects/7?panel=%2Fsettings"),
        &[("page", true)],
    );
    app.wait_for(|shown| shown.iter().any(|text| text == "at /projects/7"));
    assert!(app.shows("at /projects/7"), "shown: {:?}", app.shown);
    assert_eq!(
        app.navigation().location().unwrap(),
        "/app/projects/7?panel=%2Fsettings"
    );
}
