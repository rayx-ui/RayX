//! Navigation regions on NSH, headless: a region as an NSH navigation stack whose pages follow
//! its journal, a region as its current route alone, and `NavigationRegion` for TSX (`navigationService.region(name)`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use futures::executor::block_on;
use nsh_core::element::screen;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, prelude::*};
use nsh_engine::{App, EngineConfig, LocalEngine, MountSink};
use rayx_app::services::navigation::{NavigationArgs, NavigationService};
use rayx_app_nsh::testing;
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_navigation::{
    NavigationAppBuilderExt as _, RouteDefinition, navigation_region, navigation_view,
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
    build_app(::rayx_app::testing::add_host_fakes(RayXAppBuilder::new())).start();
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
    let navigation = services
        .required_service::<dyn NavigationService>()
        .unwrap();
    // A push adds a page over the first; the panel shows its route.
    block_on(navigation.navigate_to("/projects/1")).unwrap();
    block_on(navigation.navigate_in_region("panel", NavigationArgs::new("/settings"))).unwrap();
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
    block_on(navigation.back("root".into())).unwrap();
    block_on(navigation.close_region("panel".into())).unwrap();
    shown.clear();
    pump(&mut engine, &rx, &mut shown, |shown| {
        shown.iter().any(|text| text == "panel closed")
    });
    assert!(
        shown.iter().any(|text| text == "panel closed"),
        "shown: {shown:?}"
    );
    assert_eq!(navigation.journal("root").unwrap().current_index(), Some(0));
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { NavigationService, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addNavigation((options) => {
        options.addRoute("/").addRoute("/projects/{id}").addRoute("/settings");
      })
      .start();
    const panel = services.getRequiredService(NavigationService).region("panel");
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

#[test]
fn tsx_navigates_a_region_and_hears_its_pages() {
    let _serial = serial();
    let set = testing::set(rayx_services_navigation::register);
    assert_eq!(
        testing::run(set, TSX),
        "done Success|/settings>/projects/2|/settings|true 0|heard /settings|/settings>/projects/2|/settings|"
    );
}
