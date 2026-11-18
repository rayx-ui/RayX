//! `KeyValueHistoryStore`'s lock is one per bucket and scope in the process, whatever key-value
//! store object a history goes through: histories over separate store objects over one storage
//! (one per window's provider, one resolved off the engine thread) never overwrite each other's
//! journal; stores over unrelated storages that use one scope id take turns and both save; an
//! uncontended save over a store answering at once finishes at once.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{ApproxTextEngine, MountBatch, prelude::*};
use nsh_engine::{App, Engine, EngineConfig, MountSink};
use rayx_app::AppBuilder;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::services::storage::{KeyValueStore, StorageCapabilities, StorageError};
use rayx_app::testing::{FakeKeyValueStore, FakeOperations, add_host_fakes};
use rayx_app_nsh::{
    AppState, EngineRunner, RayXAppBuilder, ServiceProvider, StartAppBuilderExt as _, app_services,
    app_state,
};
use rayx_services_history::{
    HISTORY_BUCKET, HistoryAppBuilderExt as _, HistoryJournalV2, HistoryStore, HistoryStoreError,
    KeyValueHistoryStore,
};
use rayx_services_storage::{STORAGE_PREFIX, StorageAppBuilderExt as _};
use serde::{Deserialize, Serialize};

/// Pending once, then ready: the store's turn to answer comes after the other caller's.
#[derive(Default)]
struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// A store object over `values` that yields before each call, as one answering later does.
struct YieldingStore(Arc<FakeKeyValueStore>);

impl YieldingStore {
    fn over(values: &Arc<FakeKeyValueStore>) -> Arc<dyn KeyValueStore> {
        Arc::new(Self(Arc::clone(values)))
    }
}

impl KeyValueStore for YieldingStore {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::default()
    }

    fn get(
        &self,
        bucket: String,
        key: String,
    ) -> BoxFuture<'_, Result<Option<Vec<u8>>, StorageError>> {
        async move {
            YieldOnce::default().await;
            self.0.get(bucket, key).await
        }
        .boxed()
    }

    fn set(
        &self,
        bucket: String,
        key: String,
        value: Vec<u8>,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        async move {
            YieldOnce::default().await;
            self.0.set(bucket, key, value).await
        }
        .boxed()
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool, StorageError>> {
        async move {
            YieldOnce::default().await;
            self.0.remove(bucket, key).await
        }
        .boxed()
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>, StorageError>> {
        async move {
            YieldOnce::default().await;
            self.0.keys(bucket).await
        }
        .boxed()
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<(), StorageError>> {
        async move {
            YieldOnce::default().await;
            self.0.clear(bucket).await
        }
        .boxed()
    }
}

#[test]
fn history_stores_over_two_store_objects_over_one_storage_share_the_lock() {
    let values = FakeKeyValueStore::new();
    let first = KeyValueHistoryStore::new(YieldingStore::over(&values));
    let second = KeyValueHistoryStore::new(YieldingStore::over(&values));
    let scope = HistoryScopeId::new("scope-lock:two-objects").unwrap();
    let mine = HistoryJournalV2::new(1, scope.clone(), Vec::new());

    let (saved, refused) = block_on(futures::future::join(
        first.save(mine.clone(), None),
        second.save(HistoryJournalV2::new(7, scope.clone(), Vec::new()), None),
    ));

    assert_eq!(saved, Ok(()));
    assert_eq!(refused, Err(HistoryStoreError::Conflict));
    assert_eq!(block_on(second.load(&scope)).unwrap(), Some(mine));
}

#[test]
fn history_stores_over_unrelated_storages_with_one_scope_id_both_save() {
    let first = KeyValueHistoryStore::new(YieldingStore::over(&FakeKeyValueStore::new()));
    let second = KeyValueHistoryStore::new(YieldingStore::over(&FakeKeyValueStore::new()));
    let scope = HistoryScopeId::new("scope-lock:unrelated").unwrap();

    let (a, b) = block_on(futures::future::join(
        first.save(HistoryJournalV2::new(1, scope.clone(), Vec::new()), None),
        second.save(HistoryJournalV2::new(2, scope.clone(), Vec::new()), None),
    ));

    assert_eq!((a, b), (Ok(()), Ok(())));
    assert_eq!(block_on(first.load(&scope)).unwrap().unwrap().revision, 1);
    assert_eq!(block_on(second.load(&scope)).unwrap().unwrap().revision, 2);
}

#[test]
fn an_uncontended_save_over_a_store_answering_at_once_finishes_at_once() {
    let values: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let store = KeyValueHistoryStore::new(values);
    let scope = HistoryScopeId::new("scope-lock:at-once").unwrap();
    store
        .save(HistoryJournalV2::new(1, scope.clone(), Vec::new()), None)
        .now_or_never()
        .expect("saves at once")
        .unwrap();
    assert!(
        store
            .delete_scope(&scope, 1)
            .now_or_never()
            .expect("deletes at once")
            .unwrap()
    );
}

/// The ready roots of the surfaces, as each engine's thread starts its app.
static ROOTS: Mutex<Vec<ServiceProvider>> = Mutex::new(Vec::new());

#[component]
fn Root() -> impl IntoElement {
    add_host_fakes(with_history(RayXAppBuilder::new())).start();
    dynamic(move || match app_state() {
        AppState::Ready => {
            ROOTS.lock().unwrap().push(app_services().unwrap());
            text("ready").into_element()
        }
        AppState::Failed(message) => text(format!("failed: {message}")).into_element(),
        _ => text("starting").into_element(),
    })
}

/// RayX's NSH store and persistent history, with fake operations.
fn with_history<B: AppBuilder>(builder: B) -> B {
    let mut builder = builder.add_storage().add_persistent_history();
    builder
        .services_mut()
        .add_singleton_as::<dyn OperationService>(
            FakeOperations::new() as Arc<dyn OperationService>
        );
    builder
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
    const KIND: &'static str = "history.scope-lock.set";
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
            ["scope-lock.value"],
            before,
            operation.0,
        ))
    }
}

/// An app's operations, with its target bound to `scope`.
fn bound(
    root: &ServiceProvider,
    scope: &HistoryScopeId,
) -> (Arc<dyn OperationService>, OperationTargetId) {
    let operations = root.required_service::<dyn OperationService>().unwrap();
    let history = root.required_service::<dyn HistoryService>().unwrap();
    let target_id = OperationTargetId::new("scope-lock:target").unwrap();
    block_on(operations.register_handler(target_id.clone(), Arc::new(Target(Mutex::new(0)))))
        .unwrap();
    block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
    (operations, target_id)
}

#[test]
fn a_store_resolved_off_the_engine_thread_shares_the_lock() {
    let device = Arc::new(Device::default());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let _surface = Engine::start(
        EngineConfig {
            storage: Some(Arc::clone(&device) as Arc<dyn StorageBackend>),
            ..EngineConfig::new(App::Rust(nsh_core::element::Root::of::<Root>()))
        },
        || ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while ROOTS.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let surface_root = ROOTS
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("the app started");

    // A second app over the surface's NSH storage, its store resolved and started here, off the
    // engine thread (its calls go through the surface's engine runner).
    let mut builder = add_host_fakes(with_history(rayx_app::RayXAppBuilder::new()));
    builder
        .services_mut()
        .replace_service((*surface_root.required_service::<EngineRunner>().unwrap()).clone());
    let off_engine = builder.build().unwrap();
    block_on(
        off_engine
            .startup_registry()
            .initialize(StartupInitializationContext::new(off_engine.clone())),
    )
    .unwrap();

    let scope = HistoryScopeId::new("scope-lock:off-engine").unwrap();
    let edits: Vec<_> = [&surface_root, &off_engine]
        .into_iter()
        .map(|root| bound(root, &scope))
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
