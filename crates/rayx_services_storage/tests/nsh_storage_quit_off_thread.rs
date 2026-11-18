//! What the app saves off the engine thread as it quits reaches the device, headless: a
//! `Destroying` subscriber (the event published from NSH's quit listener, as RayX's suspension
//! service does) hands the save to a thread of its own, which calls RayX's store; the host runs
//! what that thread queues for the engine before the engine stops, so the store writes NSH's
//! storage and NSH's backend keeps it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use nsh_core::element::desktop::on_app_quit;
use nsh_core::element::reactive::create_root;
use nsh_core::platform::{Idiom, Os};
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, Environment, MountSink};
use rayx_app::AppBuilder as _;
use rayx_app::services::storage::{KeyValueStore, StorageError};
use rayx_app::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService,
};
use rayx_app::testing::{FakeSuspension, add_host_fakes};
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_storage::StorageAppBuilderExt as _;

/// The ready root, for the test's thread (the threaded engine runs the app on its own).
static ROOT: Mutex<Option<ServiceProvider>> = Mutex::new(None);
/// The thread the `Destroying` subscriber handed the save to, and what the save answered.
static SAVER: Mutex<Option<JoinHandle<Result<(), StorageError>>>> = Mutex::new(None);

/// The app's suspension service.
fn suspension() -> Arc<FakeSuspension> {
    static SUSPENSION: OnceLock<Arc<FakeSuspension>> = OnceLock::new();
    Arc::clone(SUSPENSION.get_or_init(FakeSuspension::new))
}

fn build_app(mut builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .services_mut()
        .add_singleton_as::<dyn SuspensionService>(suspension() as Arc<dyn SuspensionService>);
    add_host_fakes(builder.add_storage())
}

#[component]
fn Root() -> impl IntoElement {
    build_app(RayXAppBuilder::new()).start();
    // What RayX's suspension service does as NSH quits the app (in a root of its own, never
    // disposed, as it does).
    Owner::detached().run(|| {
        create_root(|| {
            on_app_quit(|| {
                let suspension = suspension();
                suspension.emit(SuspensionEvent::new(
                    SuspensionEventKind::Destroying,
                    SuspensionEventReason::HostStopping,
                ));
                suspension.emit(SuspensionEvent::new(
                    SuspensionEventKind::Destroyed,
                    SuspensionEventReason::HostStopped,
                ));
                std::future::ready(())
            });
        });
    });
    dynamic(move || match app_state() {
        AppState::Ready => {
            let root = app_services().unwrap();
            let store = root.required_service::<dyn KeyValueStore>().unwrap();
            suspension()
                .events()
                .subscribe(move |event| {
                    // Saved off the engine thread as the app quits.
                    if event.kind == SuspensionEventKind::Destroying {
                        let store = Arc::clone(&store);
                        *SAVER.lock().unwrap() = Some(std::thread::spawn(move || {
                            futures::executor::block_on(store.set(
                                "draft".into(),
                                "text".into(),
                                b"unsaved".to_vec(),
                            ))
                        }));
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
fn a_save_made_off_the_engine_thread_as_the_app_quits_reaches_the_device() {
    let device = Arc::new(Device::default());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(
        EngineConfig {
            environment: Environment {
                os: Os::MacOs,
                idiom: Idiom::Desktop,
                ..Environment::default()
            },
            storage: Some(device.clone() as Arc<dyn StorageBackend>),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        || ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while ROOT.lock().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ROOT.lock().unwrap().is_some(), "the app started");
    assert!(!device.0.lock().unwrap().contains_key("rayx/draft/text"));

    // The engine stops (the last window closed, the system ended the app).
    drop(engine);

    let saver = SAVER.lock().unwrap().take().expect("Destroying was heard");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !saver.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(saver.is_finished(), "the save answered");
    assert_eq!(saver.join().unwrap(), Ok(()));
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
