//! The platform service: snapshots, RayX's service publishing only real changes of NSH's
//! environment (headless, on NSH), and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::desktop::{WindowPlacement, WindowState};
use nsh_core::geometry::{Bounds, Point, Points, Size};
use nsh_core::platform::{ColorScheme as NshColorScheme, Idiom, Os};
use nsh_core::prelude::{IntoElement, component, dynamic, text};
use nsh_core::{ApproxTextEngine, MountBatch};
use nsh_engine::{App, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::services::platform::*;
use rayx_app::testing::{FakePlatform, add_host_fakes};
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_app_nsh::{
    AppState, RayXAppBuilder as NshAppBuilder, ServiceProvider, StartAppBuilderExt as _,
    app_services, app_state,
};
use rayx_services_platform::PlatformAppBuilderExt as _;

#[test]
fn snapshots_describe_desktops_and_handhelds() {
    let desktop = PlatformSnapshot::default();
    assert!(desktop.desktop);
    assert_eq!(desktop.input.pointer, PointerCapability::Fine);
    assert_eq!(desktop.orientation(), Orientation::Landscape);
    let phone = PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 390.0,
            height: 844.0,
        },
        FormFactor::Phone,
    );
    assert!(!phone.desktop);
    assert!(phone.input.touch);
    assert_eq!(phone.input.keyboard, KeyboardCapability::Virtual);
    assert_eq!(phone.orientation(), Orientation::Portrait);
}

thread_local! {
    static ROOT: RefCell<Option<ServiceProvider>> = const { RefCell::new(None) };
}

#[component]
fn Root() -> impl IntoElement {
    add_host_fakes(NshAppBuilder::new().add_platform()).start();
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

/// A desktop in `color_scheme` whose window, the same each time, was last restored at
/// `restored_x` (a placement NSH tells and the snapshot leaves out).
fn desktop(restored_x: f32, color_scheme: NshColorScheme) -> Environment {
    let frame = Bounds::new(
        Point::new(Points(100.0), Points(100.0)),
        Size::new(Points(800.0), Points(600.0)),
    );
    let mut placement = WindowPlacement::default();
    placement.bounds = Bounds::new(Point::new(Points(restored_x), Points(100.0)), frame.size);
    let mut window = WindowState::new(frame, placement);
    window.focused = true;
    Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        color_scheme,
        window_state: Some(window),
        ..Environment::default()
    }
}

/// Pumps `engine` `times` times, or until `until` holds.
fn pump(
    engine: &mut LocalEngine<ApproxTextEngine>,
    rx: &Receiver<MountBatch>,
    times: usize,
    until: impl Fn() -> bool,
) -> bool {
    for _ in 0..times {
        engine.pump();
        while rx.recv_timeout(Duration::from_millis(5)).is_ok() {}
        if until() {
            return true;
        }
    }
    false
}

#[test]
fn updates_notify_only_real_changes() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            environment: desktop(0.0, NshColorScheme::Light),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    assert!(pump(&mut engine, &rx, 2_000, || ROOT
        .with_borrow(Option::is_some)));
    let root = ROOT.with_borrow(|root| root.clone()).unwrap();
    let platform = root.required_service::<dyn PlatformInfoService>().unwrap();
    // NSH's environment as the platform tells it once the app runs, and what NSH answers once
    // the app starts, have settled.
    engine
        .control()
        .set_environment(desktop(0.0, NshColorScheme::Light));
    pump(&mut engine, &rx, 50, || false);
    let before = platform.platform().get();
    assert_eq!(before.color_scheme, ColorScheme::Light);

    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let subscription = platform.platform().subscribe(move |snapshot| {
        sink.lock().unwrap().push(snapshot.color_scheme);
    });
    // NSH's environment changes, the snapshot does not: the window was restored elsewhere, then
    // the environment is told again as it is.
    engine
        .control()
        .set_environment(desktop(40.0, NshColorScheme::Light));
    pump(&mut engine, &rx, 50, || false);
    engine
        .control()
        .set_environment(desktop(40.0, NshColorScheme::Light));
    pump(&mut engine, &rx, 50, || false);
    assert!(heard.lock().unwrap().is_empty(), "nothing changed");
    assert!(Arc::ptr_eq(&before, &platform.platform().get()));

    // A real change is published once.
    engine
        .control()
        .set_environment(desktop(40.0, NshColorScheme::Dark));
    assert!(pump(&mut engine, &rx, 2_000, || {
        platform.platform().get().color_scheme == ColorScheme::Dark
    }));
    pump(&mut engine, &rx, 50, || false);
    assert_eq!(*heard.lock().unwrap(), [ColorScheme::Dark]);

    // Until the subscriber stops.
    drop(subscription);
    engine
        .control()
        .set_environment(desktop(40.0, NshColorScheme::Light));
    assert!(pump(&mut engine, &rx, 2_000, || {
        platform.platform().get().color_scheme == ColorScheme::Light
    }));
    assert_eq!(*heard.lock().unwrap(), [ColorScheme::Dark]);
    assert_eq!(platform.platform().subscribers(), 0);
}

#[test]
fn snapshots_round_trip_as_json() {
    let mut snapshot = PlatformSnapshot::default();
    snapshot.locale.languages = vec!["hr-HR".into(), "en".into()];
    snapshot.locale.text_direction = TextDirection::Ltr;
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_str::<PlatformSnapshot>(&json).unwrap(),
        snapshot
    );
}

#[test]
fn add_platform_registers_one_service_per_root() {
    let services = RayXAppBuilder::new().add_platform().build().unwrap();
    // The service initializes at startup, where it starts following NSH's environment.
    assert_eq!(services.startup_registry().services().len(), 1);
    let platform = services
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    platform.platform().publish(PlatformSnapshot {
        font_scale: 1.5,
        ..PlatformSnapshot::default()
    });
    let scoped = services
        .create_scope()
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    assert!(Arc::ptr_eq(&platform, &scoped));
    assert_eq!(scoped.platform().get().font_scale, 1.5);

    // Each root has its own.
    let other = RayXAppBuilder::new()
        .add_platform()
        .build()
        .unwrap()
        .required_service::<dyn PlatformInfoService>()
        .unwrap();
    assert!(!Arc::ptr_eq(&platform, &other));
    assert_eq!(other.platform().get().font_scale, 1.0);
}

#[test]
fn add_platform_replaces_an_apps_earlier_platform_service() {
    let custom = FakePlatform::new(PlatformSnapshot::handheld(
        ViewportSnapshot {
            width: 1.0,
            height: 2.0,
        },
        FormFactor::Tablet,
    ));
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn PlatformInfoService>(custom as Arc<dyn PlatformInfoService>);
    let services = builder.add_platform().add_platform().build().unwrap();
    assert_eq!(services.startup_registry().services().len(), 1);
    assert_eq!(
        services
            .required_service::<dyn PlatformInfoService>()
            .unwrap()
            .platform()
            .get()
            .form_factor,
        FormFactor::Desktop
    );
}
