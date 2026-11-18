//! The surface's engine runner stops cancellable work whose caller is gone, headless: a
//! `run_cancellable` whose future the caller dropped (a timer racing a call that already
//! answered, as history's call timeouts do) stops running on the engine, so the app's quit
//! doesn't wait for it and ends after its quiet window, not at its limit. (`run` work runs to
//! its end: a command fired and forgotten still happens.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app_nsh::{
    AppState, EngineRunner, RayXAppBuilder, StartAppBuilderExt as _, app_services, app_state,
};

/// The surface's runner, once the app is ready.
static RUNNER: Mutex<Option<EngineRunner>> = Mutex::new(None);
/// Whether the long timer's work started on the engine.
static STARTED: AtomicBool = AtomicBool::new(false);
/// Whether it ran to its end.
static ENDED: AtomicBool = AtomicBool::new(false);

#[component]
fn Root() -> impl IntoElement {
    ::rayx_app::testing::add_host_fakes(RayXAppBuilder::new()).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            let runner = (*app_services()
                .unwrap()
                .required_service::<EngineRunner>()
                .unwrap())
            .clone();
            *RUNNER.lock().unwrap() = Some(runner);
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

#[test]
fn work_whose_caller_dropped_it_stops_and_does_not_hold_the_quit() {
    let (tx, _rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(
        EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>())),
        || ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while RUNNER.lock().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let runner = RUNNER.lock().unwrap().clone().expect("the app started");

    // A ten-second timer whose caller gives up at once, as a call timeout does once the call
    // answered.
    drop(runner.run_cancellable(|| async {
        STARTED.store(true, Ordering::SeqCst);
        timer(Duration::from_secs(10)).await;
        ENDED.store(true, Ordering::SeqCst);
    }));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !STARTED.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        STARTED.load(Ordering::SeqCst),
        "the work started on the engine"
    );

    // The quit ends after its 50 ms quiet window, not at its 150 ms limit.
    let quitting = Instant::now();
    drop(engine);
    let quit = quitting.elapsed();
    assert!(quit < Duration::from_millis(130), "the quit took {quit:?}");
    assert!(!ENDED.load(Ordering::SeqCst));
}
