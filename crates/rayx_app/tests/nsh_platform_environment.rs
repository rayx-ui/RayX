//! What NSH 0.5's environment tells reaches the platform service, headless: the platform's name
//! (a platform built outside NSH included), a TV's remote input, the keyboard's animation, Apple's
//! shortcuts in a browser, the screen readers' active descendants, a terminal's cell, and what
//! the platform can do (shaders, the screen color picker, and the web views and speech it
//! answers asynchronously).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::geometry::{Points, Size};
use nsh_core::platform::{Idiom, Os};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::rayx_runtime::services::platform::{
    FormFactor, KeyboardCapability, PlatformAppBuilderExt, PlatformFamily, PlatformInfoService,
    PlatformSnapshot, PointerCapability, ViewportSnapshot,
};
use rayx_app::{AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, set_app};

static ROOT: Mutex<Option<ServiceProvider>> = Mutex::new(None);

fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *ROOT.lock().unwrap() = None;
    guard
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_platform()
}

#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    rayx_app::use_platform_sync();
    dynamic(move || match app_state() {
        AppState::Ready => {
            *ROOT.lock().unwrap() = Some(app_services().unwrap());
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

fn pump_until(
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

/// The platform service's snapshot of an app started in `environment`, once `until` holds for
/// it.
fn snapshot_in(
    environment: Environment,
    until: impl Fn(&PlatformSnapshot) -> bool,
) -> PlatformSnapshot {
    *ROOT.lock().unwrap() = None;
    set_app(build_app);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut engine = LocalEngine::start(
        EngineConfig {
            environment,
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let current = || {
        ROOT.lock().unwrap().clone().map(|root| {
            root.required_service::<PlatformInfoService>()
                .unwrap()
                .current()
        })
    };
    pump_until(&mut engine, &rx, || {
        current().is_some_and(|snapshot| until(&snapshot))
    });
    current().unwrap()
}

#[test]
fn a_tv_is_driven_by_its_remote() {
    let _serial = serial();
    let snapshot = snapshot_in(
        Environment {
            os: Os::Tizen,
            idiom: Idiom::Tv,
            ..Environment::default()
        },
        |snapshot| snapshot.family == PlatformFamily::Tizen,
    );
    assert_eq!(snapshot.platform_name, "tizen");
    assert_eq!(snapshot.form_factor, FormFactor::Tv);
    assert!(!snapshot.desktop);
    assert_eq!(snapshot.input.pointer, PointerCapability::None);
    assert_eq!(snapshot.input.keyboard, KeyboardCapability::Virtual);
    assert!(snapshot.input.remote);
    assert!(!snapshot.input.touch);
}

#[test]
fn a_platform_built_outside_nsh_names_itself_and_tells_what_it_can_do() {
    let _serial = serial();
    let snapshot = snapshot_in(
        Environment {
            os: Os::Android,
            idiom: Idiom::Phone,
            platform: Some("miniprogram"),
            keyboard_duration: 250.0,
            reads_active_descendants: true,
            screen_color: true,
            ..Environment::default()
        },
        |snapshot| snapshot.platform_name == "miniprogram",
    );
    assert_eq!(snapshot.family, PlatformFamily::Android);
    assert!(!snapshot.catalyst);
    assert_eq!(snapshot.keyboard_animation_millis, 250.0);
    assert!(snapshot.reads_active_descendants);
    assert!(snapshot.features.screen_color_picker);
    assert_eq!(snapshot.terminal_cell, None);
}

#[test]
fn a_terminal_has_its_cell_and_a_browser_on_a_mac_apple_s_shortcuts() {
    let _serial = serial();
    let terminal = snapshot_in(
        Environment {
            os: Os::Linux,
            idiom: Idiom::Terminal,
            cell: Some(Size::new(Points(8.0), Points(16.0))),
            ..Environment::default()
        },
        |snapshot| snapshot.terminal_cell.is_some(),
    );
    assert_eq!(
        terminal.terminal_cell,
        Some(ViewportSnapshot {
            width: 8.0,
            height: 16.0
        })
    );
    assert!(terminal.desktop);

    let browser = snapshot_in(
        Environment {
            os: Os::Web,
            idiom: Idiom::Desktop,
            apple_keyboard: true,
            ..Environment::default()
        },
        |snapshot| snapshot.family == PlatformFamily::Web,
    );
    assert!(browser.apple_keyboard);
    assert_eq!(browser.platform_name, "web");
}

#[test]
fn what_the_platform_answers_of_web_views_and_speech_reaches_the_service() {
    let _serial = serial();
    let environment = Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        web_views: true,
        shaders: true,
        ..Environment::default()
    };
    // The platform shows web views: NSH answers it asynchronously, after the app is ready.
    let snapshot = snapshot_in(environment, |snapshot| snapshot.features.web_views);
    assert!(snapshot.features.web_views);
    assert!(snapshot.features.shaders);
}
