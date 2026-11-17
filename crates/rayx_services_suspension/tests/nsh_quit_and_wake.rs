//! The app's quitting and the system's wakes on NSH, headless: while the app asks before it
//! quits, NSH's Quit item comes to it (`QuitRequested`) and the suspension service's `quit()`
//! quits; as the app quits the service hears `Destroying` then `Destroyed` (what the storage
//! writes then is `rayx_services_storage`'s group); a wake from sleep is `Woke`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use futures::FutureExt as _;
use nsh_core::desktop::{DesktopCommand, WindowEvent};
use nsh_core::platform::{Idiom, Os};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, MountBatch, MountItem, ROOT_ID, prelude::*,
};
use nsh_engine::{App, Engine, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::services::suspension::{SuspensionEventKind, SuspensionService};
use rayx_app::testing::add_host_fakes;
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_suspension::SuspensionAppBuilderExt as _;

/// The ready root, for the test's thread (the threaded engine runs the app on its own).
static ROOT: Mutex<Option<ServiceProvider>> = Mutex::new(None);
/// The suspension events the app heard, in order.
static HEARD: Mutex<Vec<SuspensionEventKind>> = Mutex::new(Vec::new());

/// The tests share the app (one composition root per process): one at a time.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *ROOT.lock().unwrap() = None;
    HEARD.lock().unwrap().clear();
    guard
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    add_host_fakes(builder.add_suspension())
}

#[component]
fn Root() -> impl IntoElement {
    build_app(RayXAppBuilder::new()).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            let root = app_services().unwrap();
            let suspension = root.required_service::<dyn SuspensionService>().unwrap();
            suspension
                .events()
                .subscribe(move |event| HEARD.lock().unwrap().push(event.kind))
                .detach();
            *ROOT.lock().unwrap() = Some(root);
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

fn desktop(wake_count: u32) -> Environment {
    Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        wake_count,
        ..Environment::default()
    }
}

fn ready_root() -> ServiceProvider {
    ROOT.lock().unwrap().clone().expect("the app started")
}

fn suspension() -> Arc<dyn SuspensionService> {
    ready_root()
        .required_service::<dyn SuspensionService>()
        .unwrap()
}

fn asks_before_quit(suspension: &Arc<dyn SuspensionService>) -> bool {
    suspension.suspension().get().asks_before_quit
}

fn wakes() -> u32 {
    suspension().suspension().get().wakes
}

fn heard() -> Vec<SuspensionEventKind> {
    HEARD.lock().unwrap().clone()
}

/// The app on a headless desktop engine, with the desktop commands it sent.
struct Running {
    engine: LocalEngine<ApproxTextEngine>,
    rx: Receiver<MountBatch>,
    commands: Vec<DesktopCommand>,
}

impl Running {
    fn start() -> Running {
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = LocalEngine::start(
            EngineConfig {
                environment: desktop(0),
                ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
            },
            ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap();
        let mut running = Running {
            engine,
            rx,
            commands: Vec::new(),
        };
        running.pump_until(|_| ROOT.lock().unwrap().is_some());
        running
    }

    /// Pumps until `done` holds for the desktop commands sent so far (or 10 s).
    fn pump_until(&mut self, done: impl Fn(&[DesktopCommand]) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            self.engine.pump();
            while let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(5)) {
                for item in batch.items {
                    if let MountItem::Command {
                        command: Command::Desktop(command),
                        ..
                    } = item
                    {
                        self.commands.push(*command);
                    }
                }
            }
            if done(&self.commands) {
                return;
            }
        }
        panic!("the condition never held: {:?}", self.commands);
    }
}

#[test]
fn an_app_that_asks_before_it_quits_hears_the_quit_item_and_quits_when_it_decides() {
    let _serial = serial();
    let mut app = Running::start();
    let suspension = suspension();
    assert!(!asks_before_quit(&suspension));

    // Asked from any thread: the platform leaves quitting to the app.
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&requests);
    let asking = suspension.on_quit_request(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    assert!(asks_before_quit(&suspension));
    app.pump_until(|commands| commands.contains(&DesktopCommand::SetQuitOnRequest(false)));

    // The user's Quit item: the app hears it and stays.
    app.engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Window,
        EventPayload::Window(Box::new(WindowEvent::QuitRequest)),
    );
    app.pump_until(|_| requests.load(Ordering::SeqCst) == 1);
    assert!(heard().contains(&SuspensionEventKind::QuitRequested));
    assert!(!app.commands.contains(&DesktopCommand::Quit));

    // The app decides, on any thread.
    suspension.quit().now_or_never().unwrap().unwrap();
    app.pump_until(|commands| commands.contains(&DesktopCommand::Quit));

    // Without a listener the Quit item quits again.
    drop(asking);
    assert!(!asks_before_quit(&suspension));
    app.pump_until(|commands| commands.contains(&DesktopCommand::SetQuitOnRequest(true)));
}

#[test]
fn a_wake_from_sleep_reaches_the_suspension_service() {
    let _serial = serial();
    let mut app = Running::start();
    assert_eq!(wakes(), 0);
    app.engine.control().set_environment(desktop(1));
    app.pump_until(|_| wakes() == 1);
    app.engine.control().set_environment(desktop(3));
    app.pump_until(|_| wakes() == 3);
    let woke = heard()
        .into_iter()
        .filter(|kind| *kind == SuspensionEventKind::Woke)
        .count();
    assert_eq!(woke, 3);
}

#[test]
fn the_app_hears_it_quit() {
    let _serial = serial();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(
        EngineConfig {
            environment: desktop(0),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        || ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while ROOT.lock().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ROOT.lock().unwrap().is_some(), "the app started");
    // The suspension service registers its quit listener as it starts, in the turn that makes
    // the app ready, and the engine learns of it as that turn ends: a wake heard in a later turn
    // shows it has.
    engine.control().set_environment(desktop(1));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while wakes() < 1 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(wakes(), 1);
    assert!(!heard().contains(&SuspensionEventKind::Destroying));

    // The engine stops (the last window closed, the system ended the app).
    drop(engine);

    let quitting: Vec<_> = heard()
        .into_iter()
        .filter(|kind| {
            matches!(
                kind,
                SuspensionEventKind::Destroying | SuspensionEventKind::Destroyed
            )
        })
        .collect();
    assert_eq!(
        quitting,
        [
            SuspensionEventKind::Destroying,
            SuspensionEventKind::Destroyed
        ]
    );
}
