//! A desktop's displays and window on NSH: the platform service has NSH's displays, the app's
//! window and the direction of the user's language; the suspension service hears the window's
//! focus and being minimized.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::desktop::{Display, DisplayId, WindowPlacement, WindowState};
use nsh_core::geometry::{Bounds, Point, Points, Size};
use nsh_core::platform::{Idiom, Locale, LocaleSettings, Os};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::rayx_runtime::services::platform::{
    PlatformAppBuilderExt, PlatformInfoService, TextDirection,
};
use rayx_app::rayx_runtime::services::suspension::{
    SuspensionAppBuilderExt, SuspensionService, WindowActivity,
};
use rayx_app::{AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, set_app};

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_platform().add_suspension()
}

#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    rayx_app::use_platform_sync();
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

fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Points> {
    Bounds::new(
        Point::new(Points(x), Points(y)),
        Size::new(Points(width), Points(height)),
    )
}

fn window(focused: bool, minimized: bool) -> WindowState {
    let frame = bounds(100.0, 100.0, 800.0, 600.0);
    let mut placement = WindowPlacement::default();
    placement.bounds = frame;
    let mut state = WindowState::new(frame, placement);
    state.focused = focused;
    state.minimized = minimized;
    state.visible = !minimized;
    state.display = Some(DisplayId(7));
    state
}

fn environment(window_state: WindowState) -> Environment {
    let locale = Locale::parse("he-IL").unwrap();
    Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        locale,
        locale_settings: LocaleSettings::of(locale),
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

#[test]
fn the_desktops_displays_and_window_reach_the_services() {
    set_app(build_app);
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
    engine.control().set_displays(vec![Display::new(
        DisplayId(7),
        bounds(0.0, 0.0, 1920.0, 1080.0),
        bounds(0.0, 25.0, 1920.0, 1055.0),
        true,
    )]);
    pump(&mut engine, &rx, || ROOT.with_borrow(Option::is_some));
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let platform = root.required_service::<PlatformInfoService>().unwrap();
    let suspension = root.required_service::<SuspensionService>().unwrap();
    pump(&mut engine, &rx, || !platform.current().displays.is_empty());

    let snapshot = platform.current();
    assert_eq!(snapshot.displays.len(), 1);
    let display = &snapshot.displays[0];
    assert_eq!(display.id, "0000000000000007");
    assert!(display.primary);
    assert_eq!(display.visible_bounds.y, 25.0);
    let shown = snapshot.window.clone().unwrap();
    assert!(shown.focused && shown.visible && !shown.minimized);
    assert_eq!(shown.bounds.width, 800.0);
    assert_eq!(snapshot.window_display().unwrap().id, "0000000000000007");
    // Hebrew is written right to left; Israel's weeks start on Sunday.
    assert_eq!(snapshot.locale.text_direction, TextDirection::Rtl);
    assert_eq!(snapshot.locale.first_day_of_week, "sunday");
    assert_eq!(
        suspension.current().window,
        Some(WindowActivity {
            focused: true,
            minimized: false,
            visible: true,
        })
    );

    // Minimizing the window reaches the suspension service.
    engine
        .control()
        .set_environment(environment(window(false, true)));
    pump(&mut engine, &rx, || {
        suspension
            .current()
            .window
            .is_some_and(|window| window.minimized)
    });
    assert_eq!(
        suspension.current().window,
        Some(WindowActivity {
            focused: false,
            minimized: true,
            visible: false,
        })
    );
    assert!(platform.current().window.unwrap().minimized);
}
