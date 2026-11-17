//! Persistent history over a key-value store that answers later, on another thread (as a TS
//! `KeyValueStore` answers in JS's next turn): scopes load when a command awaits them, saves
//! follow the recorded operations without anyone awaiting them, a second app over the same
//! store undoes the first app's edits, and an operation recorded before its scope loaded is kept
//! above the loaded journal.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
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
use rayx_services_history::{HISTORY_BUCKET, HistoryAppBuilderExt, HistoryJournalV2};
use serde::{Deserialize, Serialize};

const WAIT: Duration = Duration::from_secs(5);

/// A key-value store answering each call a few milliseconds later, from a thread of its own.
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
    const KIND: &'static str = "history.late-store.set";
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
            ["late.value"],
            before,
            operation.0,
        ))
    }
}

/// An app with persistent history over a late store of `values`, started, with a target at
/// `value` (bound to the scope `late:scope` when `bound`, else in its own scope).
struct App {
    _services: ServiceProvider,
    operations: Arc<dyn OperationService>,
    history: Arc<dyn HistoryService>,
    target: Arc<Target>,
    target_id: OperationTargetId,
    scope: HistoryScopeId,
}

impl App {
    fn start(values: &Arc<FakeKeyValueStore>, value: u64, bound: bool) -> Self {
        let mut builder = RayXAppBuilder::new();
        let services = builder.services_mut();
        services.add_singleton_as::<dyn KeyValueStore>(
            Arc::new(LateStore(Arc::clone(values))) as Arc<dyn KeyValueStore>
        );
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
        let target = Arc::new(Target(Mutex::new(value)));
        let target_id = OperationTargetId::new("late:target").unwrap();
        block_on(operations.register_handler(target_id.clone(), target.clone())).unwrap();
        let scope = if bound {
            let scope = HistoryScopeId::new("late:scope").unwrap();
            block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
            scope
        } else {
            HistoryScopeId::new(target_id.as_str()).unwrap()
        };
        Self {
            _services: services,
            operations,
            history,
            target,
            target_id,
            scope,
        }
    }

    fn set(&self, value: u64) {
        block_on(
            self.operations
                .submit(&self.target_id, Set(value), OperationOrigin::LocalUser),
        )
        .unwrap();
    }

    fn value(&self) -> u64 {
        *self.target.0.lock().unwrap()
    }
}

/// Waits until the journal of `scope` in `values` has `entries` entries.
fn saved(values: &FakeKeyValueStore, scope: &HistoryScopeId, entries: usize) {
    let deadline = Instant::now() + WAIT;
    loop {
        let journal = block_on(values.get(HISTORY_BUCKET.into(), scope.as_str().into()))
            .unwrap()
            .map(|bytes| HistoryJournalV2::decode(&bytes).unwrap());
        if journal.is_some_and(|journal| journal.entries.len() == entries) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the journal of {scope:?} never had {entries} entries"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits until `app`'s scope has the depths `depths`.
fn depths(app: &App, depths: (usize, usize)) {
    let deadline = Instant::now() + WAIT;
    while app.history.stack_depths(&app.scope) != depths {
        assert!(Instant::now() < deadline, "the scope never had {depths:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_second_app_over_a_late_store_undoes_the_first_app_s_edits() {
    let values = FakeKeyValueStore::new();
    let first = App::start(&values, 0, true);
    first.set(1);
    first.set(2);
    assert_eq!(first.history.stack_depths(&first.scope), (2, 0));
    saved(&values, &first.scope, 2);
    drop(first);

    let second = App::start(&values, 2, true);
    assert_eq!(second.history.stack_depths(&second.scope), (2, 0));
    assert!(block_on(second.history.undo(second.scope.clone())).unwrap());
    assert_eq!(second.value(), 1);
    saved(&values, &second.scope, 1);
    assert!(block_on(second.history.undo(second.scope.clone())).unwrap());
    assert_eq!(second.value(), 0);
    assert!(!block_on(second.history.undo(second.scope.clone())).unwrap());
}

#[test]
fn an_edit_recorded_before_its_scope_loaded_is_kept_above_the_journal() {
    let values = FakeKeyValueStore::new();
    let first = App::start(&values, 0, false);
    first.set(1);
    first.set(2);
    saved(&values, &first.scope, 2);
    drop(first);

    // Its scope is the target's, loaded by nothing before this edit is recorded.
    let second = App::start(&values, 2, false);
    second.set(3);
    saved(&values, &second.scope, 3);
    depths(&second, (3, 0));
    for value in [2, 1, 0] {
        assert!(block_on(second.history.undo(second.scope.clone())).unwrap());
        assert_eq!(second.value(), value);
    }
}
