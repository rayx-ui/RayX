//! History commands given up on mid-way, over a store and an operation service that start their
//! work when called and answer later (as TS implementations do): a clear or an undo dropped
//! while its save is pending leaves the store in step with the history, an undo dropped while
//! its commit is pending still moves the stacks, two histories saving the same scope in one
//! store keep both's entries, a redo racing a new edit is refused before it commits, an edit
//! recorded while a redo's commit is pending stays above it, and the edits recorded before the
//! app's store is attached are saved once it is.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::AppBuilder as _;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app::services::history::{HistoryError, HistoryEvent, HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationError, OperationEvent, OperationOrigin,
    OperationReceipt, OperationRecord, OperationService, OperationState, OperationTargetHandle,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::testing::FakeOperations;
use rayx_app::{EventStream, RayXAppBuilder, StateCell};
use rayx_services_history::{
    HistoryAppBuilderExt, HistoryJournalV2, HistoryStore, HistoryStoreError, InMemoryHistoryStore,
};
use serde::{Deserialize, Serialize};

const WAIT: Duration = Duration::from_secs(5);

fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

/// Polls `future` once, waking nobody.
fn poll_once<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    future.poll_unpin(&mut Context::from_waker(Waker::noop()))
}

/// Waits until `done` holds.
fn eventually(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "never: {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Holds work until it is opened: before a call (`pass`), or a call started already, from a
/// thread of its own (`later`).
#[derive(Default)]
struct Gate {
    held: AtomicBool,
    waiting: Mutex<Vec<Waker>>,
}

impl Gate {
    fn hold(&self) {
        self.held.store(true, Ordering::SeqCst);
    }

    fn open(&self) {
        self.held.store(false, Ordering::SeqCst);
        for waker in std::mem::take(&mut *self.waiting.lock().unwrap()) {
            waker.wake();
        }
    }

    async fn pass(&self) {
        futures::future::poll_fn(|cx| {
            if !self.held.load(Ordering::SeqCst) {
                return Poll::Ready(());
            }
            self.waiting.lock().unwrap().push(cx.waker().clone());
            if self.held.load(Ordering::SeqCst) {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
    }

    /// Runs `work` now on a thread of its own once the gate is open, whoever awaits it.
    fn later<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> BoxFuture<'static, T> {
        let (send, receive) = futures::channel::oneshot::channel();
        let gate = Arc::clone(self);
        std::thread::spawn(move || {
            while gate.held.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = send.send(work());
        });
        async move { receive.await.expect("the work ran") }.boxed()
    }
}

/// A store whose saves start when called and write once its gate opens.
struct LateStore {
    inner: InMemoryHistoryStore,
    gate: Arc<Gate>,
}

impl AsyncInitializable for LateStore {
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

impl StartupInitializable for LateStore {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }
}

impl HistoryStore for LateStore {
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
        let inner = self.inner.clone();
        self.gate
            .later(move || now(inner.save(journal, expected_revision)))
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>> {
        self.inner.delete_scope(scope)
    }
}

/// `rayx_app`'s fake, whose `submit_record` waits at `submits` and whose `commit` starts when
/// called and commits once `commits` opens.
struct Late {
    inner: Arc<FakeOperations>,
    submits: Arc<Gate>,
    commits: Arc<Gate>,
}

impl OperationService for Late {
    fn operations(&self) -> &StateCell<OperationState> {
        self.inner.operations()
    }

    fn events(&self) -> &EventStream<OperationEvent> {
        self.inner.events()
    }

    fn instance_id(&self) -> u64 {
        self.inner.instance_id()
    }

    fn register_target(
        &self,
        target_id: OperationTargetId,
        handler: OperationTargetHandle,
    ) -> BoxFuture<'_, Result<(), OperationError>> {
        self.inner.register_target(target_id, handler)
    }

    fn release_target(
        &self,
        target_id: OperationTargetId,
    ) -> BoxFuture<'_, Result<bool, OperationError>> {
        self.inner.release_target(target_id)
    }

    fn begin(&self) -> BoxFuture<'_, Result<u64, OperationError>> {
        self.inner.begin()
    }

    fn commit(&self, transaction_id: u64) -> BoxFuture<'_, Result<(), OperationError>> {
        let inner = Arc::clone(&self.inner);
        self.commits
            .later(move || now(inner.commit(transaction_id)))
    }

    fn abort(&self, transaction_id: u64) -> BoxFuture<'_, Result<(), OperationError>> {
        self.inner.abort(transaction_id)
    }

    fn submit_record(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
        transaction_id: Option<u64>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        async move {
            self.submits.pass().await;
            self.inner
                .submit_record(record, origin, transaction_id)
                .await
        }
        .boxed()
    }

    fn publish_remote_record(
        &self,
        record: OperationRecord,
        peer_id: String,
        before_revision: u64,
        after_revision: u64,
        affected_ids: Vec<String>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        self.inner.publish_remote_record(
            record,
            peer_id,
            before_revision,
            after_revision,
            affected_ids,
        )
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.cancellation.set";
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
            ["cancellation.value"],
            before,
            operation.0,
        ))
    }
}

struct Fixture {
    /// The fake, answering at once: the app's own edits.
    operations: Arc<dyn OperationService>,
    history: Arc<dyn HistoryService>,
    store: InMemoryHistoryStore,
    saves: Arc<Gate>,
    submits: Arc<Gate>,
    commits: Arc<Gate>,
    target: Arc<Target>,
    target_id: OperationTargetId,
    scope: HistoryScopeId,
    failures: Arc<Mutex<Vec<String>>>,
}

impl Fixture {
    fn new() -> Self {
        let inner = FakeOperations::new();
        let (saves, submits, commits) = Default::default();
        let late: Arc<dyn OperationService> = Arc::new(Late {
            inner: Arc::clone(&inner),
            submits: Arc::clone(&submits),
            commits: Arc::clone(&commits),
        });
        let store = InMemoryHistoryStore::default();
        let history = rayx_services_history::testing::history_with_store(
            Arc::clone(&late),
            Arc::new(LateStore {
                inner: store.clone(),
                gate: Arc::clone(&saves),
            }),
        );
        let failures = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&failures);
        std::mem::forget(history.subscribe(move |event| {
            if let HistoryEvent::NavigationFailed(failure) = event {
                sink.lock().unwrap().push(failure.error_code.clone());
            }
        }));
        let target = Arc::new(Target::default());
        let target_id = OperationTargetId::new("cancellation:target").unwrap();
        let scope = HistoryScopeId::new("cancellation:scope").unwrap();
        now(late.register_handler(target_id.clone(), Arc::clone(&target))).unwrap();
        block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
        Self {
            operations: inner,
            history,
            store,
            saves,
            submits,
            commits,
            target,
            target_id,
            scope,
            failures,
        }
    }

    fn set(&self, value: u64) {
        now(self
            .operations
            .submit(&self.target_id, Set(value), OperationOrigin::LocalUser))
        .unwrap();
    }

    fn value(&self) -> u64 {
        *self.target.0.lock().unwrap()
    }

    fn depths(&self) -> (usize, usize) {
        self.history.stack_depths(&self.scope)
    }

    /// How many entries the stored journal has.
    fn stored(&self) -> Option<usize> {
        now(self.store.load(&self.scope))
            .unwrap()
            .map(|journal| journal.entries.len())
    }

    /// Waits until the store holds the scope's undo stack, and checks no save failed.
    fn in_step(&self) {
        eventually("the store holds the undo stack", || {
            self.stored() == Some(self.depths().0)
        });
        assert!(
            self.failures.lock().unwrap().is_empty(),
            "{:?}",
            self.failures
        );
    }
}

#[test]
fn a_clear_given_up_while_its_save_is_pending_leaves_the_store_in_step() {
    let f = Fixture::new();
    f.set(1);
    f.in_step();
    f.saves.hold();
    let mut clear = f.history.clear_scope(f.scope.clone());
    assert!(poll_once(&mut clear).is_pending());
    drop(clear);
    f.saves.open();
    f.in_step();
    assert_eq!(f.stored(), Some(0));
    // Later saves meet the revision they expect.
    f.set(2);
    f.set(3);
    f.in_step();
    assert_eq!(f.stored(), Some(2));
}

#[test]
fn an_undo_given_up_while_its_save_is_pending_leaves_the_store_in_step() {
    let f = Fixture::new();
    f.set(1);
    f.set(2);
    f.in_step();
    f.saves.hold();
    let mut undo = f.history.undo(f.scope.clone());
    // Its commit answers from another thread; then it moves its entry and waits for its save.
    eventually("the undo moved its entry", || f.depths() == (1, 1));
    assert!(poll_once(&mut undo).is_pending());
    drop(undo);
    f.saves.open();
    f.in_step();
    assert_eq!((f.value(), f.depths()), (1, (1, 1)));
    f.set(5);
    f.in_step();
    assert_eq!(f.stored(), Some(2));
}

#[test]
fn an_undo_given_up_while_its_commit_is_pending_still_moves_the_stacks() {
    let f = Fixture::new();
    f.set(5);
    f.commits.hold();
    let mut undo = f.history.undo(f.scope.clone());
    assert!(poll_once(&mut undo).is_pending());
    drop(undo);
    f.commits.open();
    eventually("the undo moved its entry", || f.depths() == (0, 1));
    assert_eq!(f.value(), 0);
    // The inverse is not applied twice.
    assert!(!block_on(f.history.undo(f.scope.clone())).unwrap());
    assert!(block_on(f.history.redo(f.scope.clone())).unwrap());
    assert_eq!(f.value(), 5);
    f.in_step();
}

#[test]
fn a_redo_racing_a_new_edit_is_refused_before_it_commits() {
    let f = Fixture::new();
    f.set(1);
    assert!(block_on(f.history.undo(f.scope.clone())).unwrap());
    assert_eq!((f.value(), f.depths()), (0, (0, 1)));
    f.submits.hold();
    let mut redo = f.history.redo(f.scope.clone());
    assert!(poll_once(&mut redo).is_pending());
    // A new edit clears the redo stack while the redo waits.
    f.set(9);
    assert_eq!(f.depths(), (1, 0));
    f.submits.open();
    assert_eq!(block_on(redo), Err(HistoryError::StackChanged));
    // The redo's change was aborted: the edit stands, and it alone is undone.
    assert_eq!((f.value(), f.depths()), (9, (1, 0)));
    assert!(block_on(f.history.undo(f.scope.clone())).unwrap());
    assert_eq!(f.value(), 0);
}

#[test]
fn edits_recorded_before_the_store_is_attached_are_saved_once_it_is() {
    let store = InMemoryHistoryStore::default();
    let target_id = OperationTargetId::new("attach:target").unwrap();
    let scope = HistoryScopeId::new(target_id.as_str()).unwrap();
    let start = |value: u64| {
        let mut builder = RayXAppBuilder::new();
        builder
            .services_mut()
            .add_singleton_as::<dyn OperationService>(
                FakeOperations::new() as Arc<dyn OperationService>
            );
        let services = builder
            .add_history_with_store(store.clone())
            .build()
            .unwrap();
        let operations = services.required_service::<dyn OperationService>().unwrap();
        let history = services.required_service::<dyn HistoryService>().unwrap();
        let target = Arc::new(Target(Mutex::new(value)));
        now(operations.register_handler(target_id.clone(), Arc::clone(&target))).unwrap();
        (services, operations, history, target)
    };
    let initialize = |services: &rayx_app::ServiceProvider| {
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .unwrap();
    };
    let stored = || {
        now(store.load(&scope))
            .unwrap()
            .map(|journal| journal.entries.len())
    };

    // Recorded before the startup attached the store: saved when it does.
    let (services, operations, history, _target) = start(0);
    now(operations.submit(&target_id, Set(1), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(history.stack_depths(&scope), (1, 0));
    assert_eq!(stored(), None);
    initialize(&services);
    assert_eq!(stored(), Some(1));
    drop((services, operations, history));

    // Again over the saved journal: kept above it.
    let (services, operations, history, target) = start(1);
    now(operations.submit(&target_id, Set(2), OperationOrigin::LocalUser)).unwrap();
    initialize(&services);
    assert_eq!(stored(), Some(2));
    assert_eq!(history.stack_depths(&scope), (2, 0));
    for value in [1, 0] {
        assert!(block_on(history.undo(scope.clone())).unwrap());
        assert_eq!(*target.0.lock().unwrap(), value);
    }
}

#[test]
fn two_histories_sharing_a_store_keep_both_s_entries() {
    let store = InMemoryHistoryStore::default();
    let scope = HistoryScopeId::new("shared:scope").unwrap();
    let app = |value: u64| {
        let operations: Arc<dyn OperationService> = FakeOperations::new();
        let history = rayx_services_history::testing::history_with_store(
            Arc::clone(&operations),
            Arc::new(store.clone()),
        );
        let failures = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&failures);
        std::mem::forget(history.subscribe(move |event| {
            if let HistoryEvent::NavigationFailed(failure) = event {
                sink.lock().unwrap().push(failure.error_code.clone());
            }
        }));
        let target_id = OperationTargetId::new("shared:target").unwrap();
        now(operations.register_handler(target_id.clone(), Arc::new(Target(Mutex::new(value)))))
            .unwrap();
        now(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
        (operations, history, target_id, failures)
    };
    // Both loaded the scope before either saved.
    let (first, first_history, target, first_failures) = app(0);
    let (second, second_history, _, second_failures) = app(0);
    now(first.submit(&target, Set(1), OperationOrigin::LocalUser)).unwrap();
    // The second's save meets the first's journal: its entries go on top of it.
    now(second.submit(&target, Set(2), OperationOrigin::LocalUser)).unwrap();
    now(second.submit(&target, Set(3), OperationOrigin::LocalUser)).unwrap();
    let entries = || now(store.load(&scope)).unwrap().unwrap().entries.len();
    assert_eq!(entries(), 3);
    assert_eq!(second_history.stack_depths(&scope), (3, 0));
    // And the first's next save goes on top of both.
    now(first.submit(&target, Set(4), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(entries(), 4);
    assert_eq!(first_history.stack_depths(&scope), (4, 0));
    // An undo in one drops only its entry from the stored journal.
    assert!(now(second_history.undo(scope.clone())).unwrap());
    now(second.submit(&target, Set(5), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(entries(), 4);
    assert!(
        first_failures.lock().unwrap().is_empty(),
        "{first_failures:?}"
    );
    assert!(
        second_failures.lock().unwrap().is_empty(),
        "{second_failures:?}"
    );
}

#[test]
fn an_edit_recorded_while_a_redo_s_commit_is_pending_stays_above_it() {
    let f = Fixture::new();
    f.set(1);
    assert!(block_on(f.history.undo(f.scope.clone())).unwrap());
    assert_eq!((f.value(), f.depths()), (0, (0, 1)));
    f.commits.hold();
    let redo = f.history.redo(f.scope.clone());
    eventually("the redo applied", || f.value() == 1);
    // An edit after the redo applied, recorded while its commit is pending.
    f.set(9);
    f.commits.open();
    assert!(block_on(redo).unwrap());
    assert_eq!(f.depths(), (2, 0));
    // Undone in the order they happened: the edit first, then the redo.
    assert!(block_on(f.history.undo(f.scope.clone())).unwrap());
    assert_eq!(f.value(), 1);
    assert!(block_on(f.history.undo(f.scope.clone())).unwrap());
    assert_eq!(f.value(), 0);
    f.in_step();
}
