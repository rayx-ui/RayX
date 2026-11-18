//! Persistent histories of two surfaces over one NSH storage in a process (two windows, two
//! engines given the same backend), headless, over RayX's NSH store (`add_storage()`): the
//! surfaces share one key-value store, so the histories' revision checks and writes hold
//! together across them, and both editing one scope at once keep both entries.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use futures::executor::block_on;
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app::AppBuilder as _;
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::testing::{FakeOperations, add_host_fakes};
use rayx_app_nsh::{
    AppState, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services, app_state,
};
use rayx_services_history::{HISTORY_BUCKET, HistoryAppBuilderExt as _, HistoryJournalV2};
use rayx_services_storage::{STORAGE_PREFIX, StorageAppBuilderExt as _};
use serde::{Deserialize, Serialize};

/// The ready roots of the surfaces, as each engine's thread starts its app.
static ROOTS: Mutex<Vec<ServiceProvider>> = Mutex::new(Vec::new());

#[component]
fn Root() -> impl IntoElement {
    let mut builder = RayXAppBuilder::new().add_storage().add_persistent_history();
    builder
        .services_mut()
        .add_singleton_as::<dyn OperationService>(
            FakeOperations::new() as Arc<dyn OperationService>
        );
    add_host_fakes(builder).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOTS.lock().unwrap().push(app_services().unwrap());
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
    const KIND: &'static str = "history.storage-surfaces.set";
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
            ["surfaces.value"],
            before,
            operation.0,
        ))
    }
}

/// A surface's app: its operations, with its target bound to `scope`.
fn bound(
    root: &ServiceProvider,
    scope: &HistoryScopeId,
) -> (Arc<dyn OperationService>, OperationTargetId) {
    let operations = root.required_service::<dyn OperationService>().unwrap();
    let history = root.required_service::<dyn HistoryService>().unwrap();
    let target_id = OperationTargetId::new("surfaces:target").unwrap();
    block_on(operations.register_handler(target_id.clone(), Arc::new(Target(Mutex::new(0)))))
        .unwrap();
    block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
    (operations, target_id)
}

#[test]
fn two_surfaces_editing_one_scope_over_one_storage_keep_both_entries() {
    let device = Arc::new(Device::default());
    let surface = || {
        let (tx, _rx) = crossbeam_channel::unbounded();
        Engine::start(
            EngineConfig {
                storage: Some(Arc::clone(&device) as Arc<dyn StorageBackend>),
                ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
            },
            || ApproxTextEngine,
            ChannelSink(tx),
        )
        .unwrap()
    };
    let _first = surface();
    let _second = surface();
    let deadline = Instant::now() + Duration::from_secs(10);
    while ROOTS.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let roots = ROOTS.lock().unwrap().clone();
    assert_eq!(roots.len(), 2, "both surfaces started");
    let scope = HistoryScopeId::new("surfaces:document").unwrap();
    let apps: Vec<_> = roots.iter().map(|root| bound(root, &scope)).collect();

    // Both edit at once, each from a thread of its own.
    let edits: Vec<_> = apps
        .into_iter()
        .zip(1..)
        .map(|((operations, target_id), value)| {
            std::thread::spawn(move || {
                block_on(operations.submit(&target_id, Set(value), OperationOrigin::LocalUser))
                    .unwrap();
            })
        })
        .collect();
    for edit in edits {
        edit.join().unwrap();
    }

    // The device's journal holds both entries once the surfaces wrote it.
    let key = format!("{STORAGE_PREFIX}{HISTORY_BUCKET}/{}", scope.as_str());
    let entries = || {
        device.0.lock().unwrap().get(&key).map(|value| {
            let json = value.strip_prefix('t').expect("the journal is text");
            HistoryJournalV2::decode(json.as_bytes())
                .unwrap()
                .entries
                .len()
        })
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while entries() != Some(2) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(entries(), Some(2), "the journal holds both entries");
}
