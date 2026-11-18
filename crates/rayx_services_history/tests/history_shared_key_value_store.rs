//! Two `KeyValueHistoryStore`s over one key-value store (two windows, two surfaces or two app
//! instances in one process): a save's or a delete's revision check and its write hold together
//! across the instances, so neither overwrites nor deletes a journal the other wrote meanwhile;
//! two persistent histories editing one scope over a store answering later keep both entries.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use rayx_app::AppBuilder as _;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::services::storage::{KeyValueStore, StorageCapabilities, StorageError};
use rayx_app::testing::{FakeKeyValueStore, FakeOperations};
use rayx_app::{RayXAppBuilder, ServiceProvider};
use rayx_services_history::{
    HISTORY_BUCKET, HistoryAppBuilderExt, HistoryJournalV2, HistoryStore, HistoryStoreError,
    KeyValueHistoryStore,
};
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

/// A key-value store that yields before each call, as one answering in JS's next turn does.
struct YieldingStore(Arc<FakeKeyValueStore>);

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

/// Two history stores, separate instances, over one yielding key-value store.
fn two_stores() -> (KeyValueHistoryStore, KeyValueHistoryStore) {
    let values: Arc<dyn KeyValueStore> = Arc::new(YieldingStore(FakeKeyValueStore::new()));
    (
        KeyValueHistoryStore::new(Arc::clone(&values)),
        KeyValueHistoryStore::new(values),
    )
}

#[test]
fn two_stores_saving_a_new_journal_at_once_save_one_and_refuse_the_other() {
    let (first, second) = two_stores();
    let scope = HistoryScopeId::new("shared-key-value:new").unwrap();
    let mine = HistoryJournalV2::new(1, scope.clone(), Vec::new());
    let theirs = HistoryJournalV2::new(7, scope.clone(), Vec::new());

    let (saved, refused) = block_on(futures::future::join(
        first.save(mine.clone(), None),
        second.save(theirs, None),
    ));

    assert_eq!(saved, Ok(()));
    assert_eq!(refused, Err(HistoryStoreError::Conflict));
    assert_eq!(block_on(second.load(&scope)).unwrap(), Some(mine));
}

#[test]
fn a_delete_at_once_with_another_store_s_save_keeps_the_saved_journal() {
    let (first, second) = two_stores();
    let scope = HistoryScopeId::new("shared-key-value:delete").unwrap();
    block_on(first.save(HistoryJournalV2::new(1, scope.clone(), Vec::new()), None)).unwrap();
    let next = HistoryJournalV2::new(2, scope.clone(), Vec::new());

    let (saved, deleted) = block_on(futures::future::join(
        first.save(next.clone(), Some(1)),
        second.delete_scope(&scope, 1),
    ));

    assert_eq!(saved, Ok(()));
    assert_eq!(deleted, Ok(false), "the journal it expected was replaced");
    assert_eq!(block_on(second.load(&scope)).unwrap(), Some(next));
}

#[test]
fn two_stores_saving_other_scopes_at_once_save_both() {
    let (first, second) = two_stores();
    let one = HistoryScopeId::new("shared-key-value:one").unwrap();
    let other = HistoryScopeId::new("shared-key-value:other").unwrap();

    let (a, b) = block_on(futures::future::join(
        first.save(HistoryJournalV2::new(1, one.clone(), Vec::new()), None),
        second.save(HistoryJournalV2::new(1, other.clone(), Vec::new()), None),
    ));

    assert_eq!((a, b), (Ok(()), Ok(())));
    assert!(block_on(first.load(&other)).unwrap().is_some());
    assert!(block_on(second.load(&one)).unwrap().is_some());
}

/// A key-value store answering each call a few milliseconds later, from a thread of its own (as
/// a TS store answers in JS's next turn).
struct LateStore(Arc<FakeKeyValueStore>);

impl LateStore {
    fn later<T: Send + 'static>(
        &self,
        call: impl FnOnce(Arc<FakeKeyValueStore>) -> Result<T, StorageError> + Send + 'static,
    ) -> BoxFuture<'static, Result<T, StorageError>> {
        let (send, receive) = futures::channel::oneshot::channel();
        let store = Arc::clone(&self.0);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(3));
            let _ = send.send(call(store));
        });
        async move { receive.await.expect("the store answers") }.boxed()
    }
}

impl KeyValueStore for LateStore {
    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::default()
    }

    fn get(
        &self,
        bucket: String,
        key: String,
    ) -> BoxFuture<'_, Result<Option<Vec<u8>>, StorageError>> {
        self.later(move |store| block_on(store.get(bucket, key)))
    }

    fn set(
        &self,
        bucket: String,
        key: String,
        value: Vec<u8>,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        self.later(move |store| block_on(store.set(bucket, key, value)))
    }

    fn remove(&self, bucket: String, key: String) -> BoxFuture<'_, Result<bool, StorageError>> {
        self.later(move |store| block_on(store.remove(bucket, key)))
    }

    fn keys(&self, bucket: String) -> BoxFuture<'_, Result<Vec<String>, StorageError>> {
        self.later(move |store| block_on(store.keys(bucket)))
    }

    fn clear(&self, bucket: String) -> BoxFuture<'_, Result<(), StorageError>> {
        self.later(move |store| block_on(store.clear(bucket)))
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.shared-key-value.set";
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
            ["shared.value"],
            before,
            operation.0,
        ))
    }
}

/// An app with persistent history over `values`, started, its target bound to `scope`.
struct App {
    _services: ServiceProvider,
    operations: Arc<dyn OperationService>,
    target_id: OperationTargetId,
}

impl App {
    fn start(values: &Arc<dyn KeyValueStore>, scope: &HistoryScopeId) -> Self {
        let mut builder = RayXAppBuilder::new();
        let services = builder.services_mut();
        services.add_singleton_as::<dyn KeyValueStore>(Arc::clone(values));
        services.add_singleton_as::<dyn OperationService>(
            FakeOperations::new() as Arc<dyn OperationService>
        );
        let services = builder.add_persistent_history().build().unwrap();
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .unwrap();
        let operations = services.required_service::<dyn OperationService>().unwrap();
        let history = services.required_service::<dyn HistoryService>().unwrap();
        let target_id = OperationTargetId::new("shared:target").unwrap();
        block_on(operations.register_handler(target_id.clone(), Arc::new(Target(Mutex::new(0)))))
            .unwrap();
        block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
        Self {
            _services: services,
            operations,
            target_id,
        }
    }

    fn set(&self, value: u64) {
        block_on(
            self.operations
                .submit(&self.target_id, Set(value), OperationOrigin::LocalUser),
        )
        .unwrap();
    }
}

#[test]
fn two_histories_editing_one_scope_over_a_late_store_keep_both_entries() {
    let values = FakeKeyValueStore::new();
    let shared: Arc<dyn KeyValueStore> = Arc::new(LateStore(Arc::clone(&values)));
    let scope = HistoryScopeId::new("shared-key-value:apps").unwrap();
    let first = App::start(&shared, &scope);
    let second = App::start(&shared, &scope);

    // Both saves start at once: each checks the store while the other's write is on its way.
    first.set(1);
    second.set(2);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let entries = block_on(values.get(HISTORY_BUCKET.into(), scope.as_str().into()))
            .unwrap()
            .map(|bytes| HistoryJournalV2::decode(&bytes).unwrap().entries.len());
        if entries == Some(2) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the journal never held both entries: {entries:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
