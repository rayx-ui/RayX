//! A desktop's window on NSH, headless: RayX's suspension service hears the window's focus and
//! being minimized from NSH's window state.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::desktop::{WindowPlacement, WindowState};
use nsh_core::geometry::{Bounds, Point, Points, Size};
use nsh_core::platform::{Idiom, Os};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::services::suspension::{SuspensionEventKind, SuspensionService, WindowActivity};
use rayx_app::testing::add_host_fakes;
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_suspension::SuspensionAppBuilderExt as _;

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

#[component]
fn Root() -> impl IntoElement {
    add_host_fakes(RayXAppBuilder::new().add_suspension()).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOT.with_borrow_mut(|root| *root = Some(app_services().unwrap()));
            text("ready").into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

fn window(focused: bool, minimized: bool) -> WindowState {
    let frame = Bounds::new(
        Point::new(Points(100.0), Points(100.0)),
        Size::new(Points(800.0), Points(600.0)),
    );
    let mut placement = WindowPlacement::default();
    placement.bounds = frame;
    let mut state = WindowState::new(frame, placement);
    state.focused = focused;
    state.minimized = minimized;
    state.visible = !minimized;
    state
}

fn environment(window_state: WindowState) -> Environment {
    Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        window_state: Some(window_state),
        ..Environment::default()
    }
}

fn pump(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    until: impl Fn() -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        engine.pump();
        while rx.recv_timeout(Duration::from_millis(5)).is_ok() {}
        if until() {
            return;
        }
    }
    panic!("the condition never held");
}

fn window_of(suspension: &Arc<dyn SuspensionService>) -> Option<WindowActivity> {
    suspension.suspension().get().window
}

#[test]
fn the_windows_focus_and_being_minimized_reach_the_suspension_service() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            environment: environment(window(true, false)),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    pump(&mut engine, &rx, || ROOT.with_borrow(Option::is_some));
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let suspension = root.required_service::<dyn SuspensionService>().unwrap();
    let windows = Arc::new(std::sync::Mutex::new(Vec::new()));
    let heard = Arc::clone(&windows);
    let _events = suspension.events().subscribe(move |event| {
        if event.kind == SuspensionEventKind::WindowChanged {
            heard.lock().unwrap().push(event.window);
        }
    });
    pump(&mut engine, &rx, || window_of(&suspension).is_some());
    assert_eq!(
        window_of(&suspension),
        Some(WindowActivity {
            focused: true,
            minimized: false,
            visible: true,
        })
    );

    // Minimizing the window takes its focus and reaches the suspension service.
    engine
        .control()
        .set_environment(environment(window(false, true)));
    pump(&mut engine, &rx, || {
        window_of(&suspension).is_some_and(|window| window.minimized)
    });
    let minimized = WindowActivity {
        focused: false,
        minimized: true,
        visible: false,
    };
    assert_eq!(window_of(&suspension), Some(minimized));
    assert_eq!(*windows.lock().unwrap(), [Some(minimized)]);
}
