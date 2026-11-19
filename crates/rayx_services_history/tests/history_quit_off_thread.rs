//! A persistent history whose last operation commits on another thread as the app quits keeps
//! its final save, headless, over RayX's NSH store (`add_storage()`): a `Destroying` subscriber
//! (the event published from NSH's quit listener, as RayX's suspension service does) hands the
//! edit to a thread of its own; the history saves its scope through the store, whose calls from
//! that thread the host runs on the engine thread before the engine stops.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use futures::executor::block_on;
use nsh_core::element::desktop::on_app_quit;
use nsh_core::element::reactive::create_root;
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app::AppBuilder as _;
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService,
};
use rayx_app::testing::{FakeOperations, FakeSuspension, add_host_fakes};
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_history::{HISTORY_BUCKET, HistoryAppBuilderExt as _, HistoryJournalV2};
use rayx_services_storage::{STORAGE_PREFIX, StorageAppBuilderExt as _};
use serde::{Deserialize, Serialize};

/// The ready root, for the test's thread (the threaded engine runs the app on its own).
static ROOT: Mutex<Option<ServiceProvider>> = Mutex::new(None);
/// The thread the `Destroying` subscriber handed the edit to.
static EDITOR: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// The app's suspension service.
fn suspension() -> Arc<FakeSuspension> {
    static SUSPENSION: OnceLock<Arc<FakeSuspension>> = OnceLock::new();
    Arc::clone(SUSPENSION.get_or_init(FakeSuspension::new))
}

#[component]
fn Root() -> impl IntoElement {
    let mut builder = RayXAppBuilder::new().add_storage().add_persistent_history();
    let services = builder.services_mut();
    services.add_singleton_as::<dyn OperationService>(
        FakeOperations::new() as Arc<dyn OperationService>
    );
    services.add_singleton_as::<dyn SuspensionService>(suspension() as Arc<dyn SuspensionService>);
    add_host_fakes(builder).start();
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

/// The device's storage.
#[derive(Default)]
struct Device(Mutex<BTreeMap<String, String>>);

impl StorageBackend for Device {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn write(&self, change: StorageChange) -> Result<(), String> {
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
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.quit-off-thread.set";
    const VERSION: u32 = 1;
}

struct Target(Mutex<u64>);

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let before = std::mem::replace(&mut *self.0.lock().unwrap(), operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["quit.value"],
            before,
            operation.0,
        ))
    }
}

#[test]
fn an_operation_committed_on_another_thread_as_the_app_quits_is_saved() {
    let device = Arc::new(Device::default());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let engine = Engine::start(
        EngineConfig {
            storage: Some(Arc::clone(&device) as Arc<dyn StorageBackend>),
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
    let root = ROOT.lock().unwrap().clone().expect("the app started");
    let scope = HistoryScopeId::new("quit:document").unwrap();
    let operations = root.required_service::<dyn OperationService>().unwrap();
    let history = root.required_service::<dyn HistoryService>().unwrap();
    let target_id = OperationTargetId::new("quit:target").unwrap();
    block_on(operations.register_handler(target_id.clone(), Arc::new(Target(Mutex::new(0)))))
        .unwrap();
    block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
    // As the app quits, its last edit commits on another thread.
    suspension()
        .events()
        .subscribe(move |event| {
            if event.kind == SuspensionEventKind::Destroying {
                let operations = Arc::clone(&operations);
                let target_id = target_id.clone();
                *EDITOR.lock().unwrap() = Some(std::thread::spawn(move || {
                    block_on(operations.submit(&target_id, Set(1), OperationOrigin::LocalUser))
                        .unwrap();
                }));
            }
        })
        .detach();

    // The engine stops (the last window closed, the system ended the app).
    drop(engine);

    let editor = EDITOR.lock().unwrap().take().expect("Destroying was heard");
    editor.join().unwrap();
    let key = format!("{STORAGE_PREFIX}{HISTORY_BUCKET}/{}", scope.as_str());
    let entries = device.0.lock().unwrap().get(&key).map(|value| {
        let json = value.strip_prefix('t').expect("the journal is text");
        HistoryJournalV2::decode(json.as_bytes())
            .unwrap()
            .entries
            .len()
    });
    assert_eq!(entries, Some(1), "the journal holds the last edit");
}
