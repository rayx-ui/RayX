//! Navigation regions on NSH, headless: a region as an NSH navigation stack whose pages follow
//! its journal, a region as its current route alone, and `NavigationRegion` for TSX.

// Loading the compiled TSX app takes `Bundle::bytecode`.
#![allow(unsafe_code, clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::element::{Registry, screen};
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, Bundle, Engine, EngineConfig, LocalEngine, MountSink};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::navigation::{
    NavigationArgs, NavigationService, RouteDefinition,
};
use rayx_app::{
    AppServices, AppState, NavigationRegion, RayXAppBuilder, ServiceProvider, app_services,
    app_state, navigation_region, navigation_view, set_app,
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
    })
}

/// The root region as a stack of pages, and the `panel` region as its current route.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOT.with_borrow_mut(|root| *root = Some(app_services().unwrap()));
            let pages = navigation_region("root", |route| {
                screen().child(text(format!("page {}", route.path)))
            })
            .unwrap();
            let panel = navigation_view("panel", |route| {
                text(format!(
                    "panel {}",
                    route.map_or("closed".to_string(), |route| route.path.clone())
                ))
            })
            .unwrap();
            view().child(pages).child(panel).into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

/// Texts the engine mounted until `done` holds for the latest ones (or 10 s).
fn pump(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    shown: &mut Vec<String>,
    done: impl Fn(&[String]) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(5)) {
            for item in &batch.items {
                if let MountItem::Text { text, .. } = item {
                    shown.push(text.text.clone());
                }
            }
        }
        if done(shown) {
            break;
        }
    }
}

#[test]
fn a_region_s_pages_follow_its_journal() {
    let _serial = serial();
    set_app(build_app);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let mut shown = Vec::new();
    pump(&mut engine, &rx, &mut shown, |shown| {
        shown.iter().any(|text| text == "panel closed")
    });
    assert!(
        shown.iter().any(|text| text == "page /"),
        "shown: {shown:?}"
    );

    let services = ROOT
        .with_borrow(|root| root.clone())
        .expect("the app started");
    let navigation = services.required_service::<NavigationService>().unwrap();
    // A push adds a page over the first; the panel shows its route.
    navigation.navigate_to("/projects/1").unwrap();
    navigation
        .navigate_in_region("panel", NavigationArgs::new("/settings"))
        .unwrap();
    shown.clear();
    pump(&mut engine, &rx, &mut shown, |shown| {
        shown.iter().any(|text| text == "page /projects/1")
            && shown.iter().any(|text| text == "panel /settings")
    });
    assert!(
        shown.iter().any(|text| text == "page /projects/1"),
        "shown: {shown:?}"
    );
    assert!(
        shown.iter().any(|text| text == "panel /settings"),
        "shown: {shown:?}"
    );

    // Back pops it, and closing the panel empties it.
    navigation.back().unwrap();
    navigation.close_region("panel").unwrap();
    shown.clear();
    pump(&mut engine, &rx, &mut shown, |shown| {
        shown.iter().any(|text| text == "panel closed")
    });
    assert!(
        shown.iter().any(|text| text == "panel closed"),
        "shown: {shown:?}"
    );
    assert_eq!(
        navigation.journal("root").unwrap().unwrap().current_index(),
        Some(0)
    );
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { AppServices, NavigationRegion } from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const app = await AppServices.load();
    const panel = NavigationRegion.of(app, "panel");
    const heard = [];
    panel.onChange((pages) => heard.push(pages.map((page) => page.path).join(">")));
    lines.push(panel.navigate("/settings").status);
    panel.navigate("/projects/2");
    lines.push(panel.pages().map((page) => page.path).join(">"));
    // A page's onDismiss goes to the page below it.
    lines.push(panel.goTo(0).route.path);
    lines.push(`${panel.close()} ${panel.pages().length}`);
    await tick();
    lines.push(`heard ${heard.join("|")}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
            }
        }
        if last.starts_with(prefix) {
            break;
        }
    }
    last
}

#[test]
fn tsx_navigates_a_region_and_hears_its_pages() {
    let _serial = serial();
    set_app(build_app);
    let mut set = Registry::ui_set("test/app");
    set.class::<AppServices>().class::<NavigationRegion>();
    let dir = std::env::temp_dir().join(format!("rayx-app-regions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, TSX).unwrap();
    let ui_sets = vec![set];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        development: false,
        app: nsh_bundler::AppInfo::default(),
        split: false,
        ui_sets: ui_sets.iter().map(Into::into).collect(),
        asset_root_from: None,
    })
    .unwrap();
    let described: Vec<_> = ui_sets.iter().map(Into::into).collect();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &described).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    // SAFETY: compiled just now by this runtime.
    let app = App::Js(unsafe { Bundle::bytecode(bytecode) });
    let _engine = Engine::start(
        EngineConfig {
            ui_sets,
            ..EngineConfig::new(app)
        },
        || ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let last = shown(&rx, "done");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        last,
        "done Success|/settings>/projects/2|/settings|true 0|heard /settings|/settings>/projects/2|/settings|"
    );
}
