//! The app's quitting and the system's wakes on NSH, headless: while the app asks before it
//! quits, NSH's Quit item comes to it (`QuitRequested`) and the suspension service's `quit()`
//! quits; as the app quits the service hears `Destroying` then `Destroyed`, and what the app saves
//! on `Destroying` reaches the device; a wake from sleep is `Woke`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::desktop::{DesktopCommand, WindowEvent};
use nsh_core::platform::{Idiom, Os};
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, MountBatch, MountItem, ROOT_ID, prelude::*,
};
use nsh_engine::{App, Engine, EngineConfig, Environment, LocalEngine, MountSink};
use rayx_app::rayx_runtime::prelude::*;
use rayx_app::rayx_runtime::services::platform::PlatformAppBuilderExt;
use rayx_app::rayx_runtime::services::storage::KeyValueStore;
use rayx_app::rayx_runtime::services::suspension::{
    SuspensionAppBuilderExt, SuspensionEventKind, SuspensionService,
};
use rayx_app::{AppState, RayXAppBuilder, ServiceProvider, app_services, app_state, set_app};

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
    builder.add_platform().add_suspension().add_storage()
}

#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    rayx_app::use_platform_sync();
    dynamic(move || match app_state() {
        AppState::Ready => {
            let root = app_services().unwrap();
            let suspension = root.required_service::<SuspensionService>().unwrap();
            let store = root.required_service::<dyn KeyValueStore>().unwrap();
            suspension
                .subscribe_events(move |event| {
                    HEARD.lock().unwrap().push(event.kind);
                    // What an app saves as it quits.
                    if event.kind == SuspensionEventKind::Destroying {
                        store.set("draft", "text", b"unsaved".to_vec()).unwrap();
                    }
                })
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

fn suspension() -> Arc<SuspensionService> {
    ready_root()
        .required_service::<SuspensionService>()
        .unwrap()
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
        set_app(build_app);
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
    assert!(!suspension.asks_before_quit());

    // Asked from any thread: the platform leaves quitting to the app.
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&requests);
    let asking = suspension.ask_before_quit(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    assert!(suspension.asks_before_quit());
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
    suspension.quit();
    app.pump_until(|commands| commands.contains(&DesktopCommand::Quit));

    // Without a listener the Quit item quits again.
    drop(asking);
    assert!(!suspension.asks_before_quit());
    app.pump_until(|commands| commands.contains(&DesktopCommand::SetQuitOnRequest(true)));
}

#[test]
fn a_wake_from_sleep_reaches_the_suspension_service() {
    let _serial = serial();
    let mut app = Running::start();
    assert_eq!(suspension().current().wakes, 0);
    app.engine.control().set_environment(desktop(1));
    app.pump_until(|_| suspension().current().wakes == 1);
    app.engine.control().set_environment(desktop(3));
    app.pump_until(|_| suspension().current().wakes == 3);
    let woke = heard()
        .into_iter()
        .filter(|kind| *kind == SuspensionEventKind::Woke)
        .count();
    assert_eq!(woke, 3);
}

/// The device's storage.
#[derive(Default)]
struct Device(Mutex<BTreeMap<String, String>>);

impl StorageBackend for Device {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn write(&self, change: StorageChange) {
        let mut entries = self.0.lock().unwrap();
        match change {
            StorageChange::Set(key, value) => {
                entries.insert(key, value);
            }
            StorageChange::Remove(key) => {
                entries.remove(&key);
            }
            StorageChange::Clear => entries.clear(),
        }
    }
}

#[test]
fn the_app_hears_it_quit_and_what_it_saves_then_reaches_the_device() {
    let _serial = serial();
    set_app(build_app);
    let device = Arc::new(Device::default());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(
        EngineConfig {
            environment: desktop(0),
            storage: Some(device.clone() as Arc<dyn StorageBackend>),
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
    assert_eq!(
        device
            .0
            .lock()
            .unwrap()
            .get("rayx/draft/text")
            .map(String::as_str),
        Some("tunsaved")
    );
}
