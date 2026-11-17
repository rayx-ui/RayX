//! What waits to be written reaches the device as the app quits, headless: RayX's store hears
//! the app quitting through the app's `dyn SuspensionService` (here a fake that publishes
//! `Destroying` then `Destroyed` from NSH's quit listener, as RayX's suspension service does)
//! and writes then what the app saved off the engine thread as it heard `Destroying`, which the
//! stopping engine runs nothing more for.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crossbeam_channel::Sender;
use futures::FutureExt as _;
use nsh_core::element::desktop::on_app_quit;
use nsh_core::element::reactive::create_root;
use nsh_core::platform::{Idiom, Os};
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, Environment, MountSink};
use rayx_app::AppBuilder as _;
use rayx_app::services::storage::KeyValueStore;
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
                    // What an app saves as it quits, here off the engine thread, where NSH's
                    // storage is not reachable: it waits in the store.
                    if event.kind == SuspensionEventKind::Destroying {
                        let store = Arc::clone(&store);
                        std::thread::spawn(move || {
                            store
                                .set("draft".into(), "text".into(), b"unsaved".to_vec())
                                .now_or_never()
                                .unwrap()
                                .unwrap();
                        })
                        .join()
                        .unwrap();
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
fn what_the_app_saves_as_it_quits_reaches_the_device() {
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
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while ROOT.lock().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ROOT.lock().unwrap().is_some(), "the app started");
    assert!(!device.0.lock().unwrap().contains_key("rayx/draft/text"));

    // The engine stops (the last window closed, the system ended the app).
    drop(engine);

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
