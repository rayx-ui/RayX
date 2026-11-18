//! Journal entries have ids: two histories' entries that are alike stay two in a shared store,
//! an entry another history undid is not brought back by a merge,
//! an entry extended after a save whose answer was lost replaces its stored copy instead of
//! doubling it, and a journal saved before entries had ids loads (its entries get ids) and is
//! saved with them.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId, OperationTransactionMode,
};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::{FakeKeyValueStore, FakeOperations};
use rayx_services_history::{
    HISTORY_BUCKET, HistoryJournalV2, HistoryStore, HistoryStoreError, HistoryTimer,
    InMemoryHistoryStore, KeyValueHistoryStore,
};
use serde::{Deserialize, Serialize};

const WAIT: Duration = Duration::from_secs(5);

fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.ids.set";
    const VERSION: u32 = 1;
}

#[derive(Default)]
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
            ["ids.value"],
            before,
            operation.0,
        ))
    }
}

/// An app's operations and history over `store`, its target at 0 bound to `scope`.
fn app(
    store: Arc<dyn HistoryStore>,
    timer: Option<HistoryTimer>,
    target_id: &OperationTargetId,
    scope: &HistoryScopeId,
) -> (
    Arc<dyn OperationService>,
    Arc<dyn HistoryService>,
    Arc<Target>,
) {
    let operations: Arc<dyn OperationService> = FakeOperations::new();
    let history = match timer {
        Some(timer) => rayx_services_history::testing::history_with_timer(
            Arc::clone(&operations),
            Some(store),
            timer,
            Duration::from_millis(30),
        ),
        None => rayx_services_history::testing::history_with_store(Arc::clone(&operations), store),
    };
    let target = Arc::new(Target::default());
    now(operations.register_handler(target_id.clone(), Arc::clone(&target))).unwrap();
    now(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
    (operations, history, target)
}

#[test]
fn alike_entries_of_two_histories_stay_two() {
    let store = InMemoryHistoryStore::default();
    let target_id = OperationTargetId::new("ids:target").unwrap();
    let scope = HistoryScopeId::new("ids:scope").unwrap();
    let (first, first_history, _) = app(Arc::new(store.clone()), None, &target_id, &scope);
    let (second, second_history, second_target) =
        app(Arc::new(store.clone()), None, &target_id, &scope);
    // The same edit from the same state, in transaction 1 of each service: alike but two.
    for operations in [&first, &second] {
        now(operations.submit(&target_id, Set(1), OperationOrigin::LocalUser)).unwrap();
    }
    let stored = now(store.load(&scope)).unwrap().unwrap().entries;
    assert_eq!(stored.len(), 2);
    assert_ne!(stored[0].id, stored[1].id);
    // Undone in one, the other's stays.
    assert!(now(second_history.undo(scope.clone())).unwrap());
    assert_eq!(*second_target.0.lock().unwrap(), 0);
    assert_eq!(now(store.load(&scope)).unwrap().unwrap().entries.len(), 1);
    assert_eq!(first_history.stack_depths(&scope), (1, 0));
}

/// A store whose saves write at once but, while `lost` is set, never answer.
struct LostAnswers {
    inner: InMemoryHistoryStore,
    lost: AtomicBool,
}

impl AsyncInitializable for LostAnswers {
    fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        self.inner.initialize(context)
    }
}

impl StartupInitializable for LostAnswers {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }
}

impl HistoryStore for LostAnswers {
    fn load<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
        self.inner.load(scope)
    }

    fn save(
        &self,
        journal: HistoryJournalV2,
        expected_revision: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
        let saved = now(self.inner.save(journal, expected_revision));
        if self.lost.load(Ordering::SeqCst) {
            return futures::future::pending().boxed();
        }
        futures::future::ready(saved).boxed()
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>> {
        self.inner.delete_scope(scope)
    }
}

#[test]
fn an_entry_extended_after_a_lost_save_answer_replaces_its_stored_copy() {
    let store = Arc::new(LostAnswers {
        inner: InMemoryHistoryStore::default(),
        lost: AtomicBool::new(true),
    });
    let timer = HistoryTimer::new(|duration| {
        let (send, receive) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            let _ = send.send(());
        });
        async move {
            let _ = receive.await;
        }
        .boxed()
    });
    let target_id = OperationTargetId::new("ids:lost").unwrap();
    let scope = HistoryScopeId::new("ids:lost-scope").unwrap();
    let (operations, history, target) = app(
        Arc::clone(&store) as Arc<dyn HistoryStore>,
        Some(timer),
        &target_id,
        &scope,
    );
    // One transaction of two operations: the save of its first lands, its answer is lost, and
    // the second extends the entry meanwhile.
    let transaction = now(operations.begin_transaction(OperationTransactionMode::New)).unwrap();
    now(transaction.submit(&target_id, Set(1), OperationOrigin::LocalUser)).unwrap();
    now(transaction.submit(&target_id, Set(2), OperationOrigin::LocalUser)).unwrap();
    now(transaction.commit()).unwrap();
    store.lost.store(false, Ordering::SeqCst);
    let deadline = Instant::now() + WAIT;
    loop {
        let stored = now(store.inner.load(&scope)).unwrap().unwrap().entries;
        if stored.len() == 1 && stored[0].forward.len() == 2 {
            break;
        }
        assert!(stored.len() <= 1, "the entry is doubled: {stored:?}");
        assert!(Instant::now() < deadline, "never saved whole: {stored:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(history.stack_depths(&scope), (1, 0));
    assert!(now(history.undo(scope.clone())).unwrap());
    assert_eq!(*target.0.lock().unwrap(), 0);
}

#[test]
fn a_journal_saved_before_entries_had_ids_loads_and_is_saved_with_them() {
    let values = FakeKeyValueStore::new();
    let target_id = OperationTargetId::new("ids:legacy").unwrap();
    let scope = HistoryScopeId::new("ids:legacy-scope").unwrap();
    let record = |value: u64| {
        serde_json::json!({
            "target_id": target_id.as_str(), "kind": Set::KIND, "version": Set::VERSION,
            "payload": serde_json::to_vec(&Set(value)).unwrap(),
        })
    };
    // Two entries without ids, as journals were saved before.
    let entry = |transaction: u64, before: u64, after: u64| {
        serde_json::json!({
            "transaction_id": transaction, "scope_id": scope.as_str(),
            "target_id": target_id.as_str(), "label_key": Set::KIND,
            "forward": [record(after)], "inverse": [record(before)],
            "before_revision": before, "after_revision": after,
        })
    };
    let journal = serde_json::json!({
        "version": 2, "revision": 2, "scope_id": scope.as_str(),
        "entries": [entry(1, 0, 1), entry(2, 1, 2)],
    });
    now(values.set(
        HISTORY_BUCKET.into(),
        scope.as_str().into(),
        serde_json::to_vec(&journal).unwrap(),
    ))
    .unwrap();
    let store: Arc<dyn HistoryStore> = Arc::new(KeyValueHistoryStore::new(
        Arc::clone(&values) as Arc<dyn KeyValueStore>
    ));
    let (operations, history, target) = app(store, None, &target_id, &scope);
    *target.0.lock().unwrap() = 2;
    assert_eq!(history.stack_depths(&scope), (2, 0));
    assert!(now(history.undo(scope.clone())).unwrap());
    assert_eq!(*target.0.lock().unwrap(), 1);
    now(operations.submit(&target_id, Set(5), OperationOrigin::LocalUser)).unwrap();
    let saved = now(values.get(HISTORY_BUCKET.into(), scope.as_str().into()))
        .unwrap()
        .unwrap();
    let saved = HistoryJournalV2::decode(&saved).unwrap();
    assert_eq!(saved.entries.len(), 2);
    assert!(saved.entries.iter().all(|entry| !entry.id.is_empty()));
    assert_eq!(saved.entries[0].id, "legacy:0:1");
}

#[test]
fn an_entry_another_history_undid_stays_out_of_the_merged_journal() {
    let store = InMemoryHistoryStore::default();
    let target_id = OperationTargetId::new("ids:undone").unwrap();
    let scope = HistoryScopeId::new("ids:undone-scope").unwrap();
    let values = || -> Vec<u64> {
        now(store.load(&scope))
            .unwrap()
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry.forward[0].decode::<Set>().unwrap().0)
            .collect()
    };
    // An earlier run saved 1 and 2.
    {
        let (operations, _history, _target) =
            app(Arc::new(store.clone()), None, &target_id, &scope);
        for value in [1, 2] {
            now(operations.submit(&target_id, Set(value), OperationOrigin::LocalUser)).unwrap();
        }
    }
    assert_eq!(values(), [1, 2]);
    // Two sessions load it.
    let (first, first_history, first_target) =
        app(Arc::new(store.clone()), None, &target_id, &scope);
    let (second, _second_history, _) = app(Arc::new(store.clone()), None, &target_id, &scope);
    *first_target.0.lock().unwrap() = 2;
    // The first undoes 2 and edits 5.
    assert!(now(first_history.undo(scope.clone())).unwrap());
    now(first.submit(&target_id, Set(5), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(values(), [1, 5]);
    // The second, which loaded 1 and 2, edits 7: 2, undone by the first, stays out.
    now(second.submit(&target_id, Set(7), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(values(), [1, 5, 7]);
}
