//! The surface's engine runner as the app quits, headless: work another thread queues for the
//! engine after it heard the app quit (an NSH quit listener, where RayX's suspension service
//! publishes `Destroying`) runs on the engine thread before the engine stops, the work that
//! work starts too, and work queued in turn as each answers; once the engine stopped, the
//! runner answers `Err(EngineGone)`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use futures::executor::block_on;
use nsh_core::element::desktop::on_app_quit;
use nsh_core::element::reactive::create_root;
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app_nsh::{
    AppState, EngineGone, EngineRunner, RayXAppBuilder, StartAppBuilderExt as _, app_services,
    app_state,
};

/// The surface's runner, once the app is ready.
static RUNNER: Mutex<Option<EngineRunner>> = Mutex::new(None);
/// The thread that heard the app quit, and what it saw: each result, and whether the work ran
/// on the engine thread.
static WORKER: Mutex<Option<JoinHandle<Vec<String>>>> = Mutex::new(None);
/// What the posted jobs saw.
static POSTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn on_engine_thread() -> bool {
    std::thread::current().name() == Some("nsh-engine")
}

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
            *RUNNER.lock().unwrap() = Some(runner.clone());
            // The app's own quit listener (later than the host's): it hands its work to a
            // thread, which queues it for the engine a moment later.
            Owner::detached().run(|| {
                create_root(move || {
                    on_app_quit(move || {
                        let runner = runner.clone();
                        *WORKER.lock().unwrap() = Some(std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(10));
                            let mut seen = Vec::new();
                            // Each queued once the one before answered, as a store's save in
                            // steps is.
                            for step in 0..3 {
                                let ran = block_on(
                                    runner.run(move || async move { (step, on_engine_thread()) }),
                                );
                                seen.push(format!("{ran:?}"));
                            }
                            // Work the started work waits for (a task that yields first).
                            let ran = block_on(runner.run(|| async {
                                let mut yielded = false;
                                futures::future::poll_fn(|context| {
                                    if std::mem::replace(&mut yielded, true) {
                                        std::task::Poll::Ready(())
                                    } else {
                                        context.waker().wake_by_ref();
                                        std::task::Poll::Pending
                                    }
                                })
                                .await;
                                on_engine_thread()
                            }));
                            seen.push(format!("{ran:?}"));
                            let posted = runner.post(|| {
                                POSTED
                                    .lock()
                                    .unwrap()
                                    .push(format!("posted {}", on_engine_thread()));
                            });
                            seen.push(format!("{posted:?}"));
                            seen
                        }));
                        std::future::ready(())
                    });
                });
            });
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
fn work_queued_as_the_app_quits_runs_before_the_engine_stops() {
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

    // The engine stops (the last window closed, the system ended the app).
    drop(engine);

    let worker = WORKER
        .lock()
        .unwrap()
        .take()
        .expect("the app's quit was heard");
    assert_eq!(
        worker.join().unwrap(),
        [
            "Ok((0, true))",
            "Ok((1, true))",
            "Ok((2, true))",
            "Ok(true)",
            "Ok(())"
        ]
    );
    assert_eq!(*POSTED.lock().unwrap(), ["posted true"]);

    // After the engine stopped.
    assert_eq!(block_on(runner.run(|| async {})), Err(EngineGone));
    assert_eq!(runner.post(|| {}), Err(EngineGone));
}
