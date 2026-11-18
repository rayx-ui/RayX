//! Failures nobody awaits, and calls that never answer: a failed save behind an edit and the
//! failure of a command whose caller gave up are `NavigationFailed` events of the scope; a call
//! of the store or the operation service that never answers fails the command with `TimedOut`
//! after the history's timeout and frees its scope; and a lock poisoned while a save is pending
//! leaves no save unanswered.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::{BoxFuture, LocalBoxFuture};
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
use rayx_app::{EventStream, StateCell};
use rayx_services_history::{
    HistoryJournalV2, HistoryStore, HistoryStoreError, HistoryTimer, InMemoryHistoryStore,
};
use serde::{Deserialize, Serialize};

const WAIT: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_millis(50);

fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

fn poll_once<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    future.poll_unpin(&mut Context::from_waker(Waker::noop()))
}

fn eventually(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "never: {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A timer on a thread of its own.
fn thread_timer() -> HistoryTimer {
    HistoryTimer::new(|duration| {
        let (send, receive) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            let _ = send.send(());
        });
        async move {
            let _ = receive.await;
        }
        .boxed()
    })
}

/// How the store's saves go.
#[derive(Clone, Copy, PartialEq)]
enum Saves {
    Succeed,
    Fail,
    /// Each save waits until the test lets it go, then fails.
    FailLater,
    /// Each save waits until the test lets it go, then saves.
    SucceedLater,
    /// No save ever answers.
    Hang,
}

struct Store {
    inner: InMemoryHistoryStore,
    saves: Mutex<Saves>,
    released: Arc<AtomicBool>,
}

impl AsyncInitializable for Store {
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

impl StartupInitializable for Store {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }
}

impl HistoryStore for Store {
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
        let failed = || Err(HistoryStoreError::Backend("the quota is full".into()));
        match *self.saves.lock().unwrap() {
            Saves::Succeed => self.inner.save(journal, expected_revision),
            Saves::Fail => futures::future::ready(failed()).boxed(),
            later @ (Saves::FailLater | Saves::SucceedLater) => {
                let released = Arc::clone(&self.released);
                let inner = self.inner.clone();
                futures::future::poll_fn(move |cx| {
                    if released.load(Ordering::SeqCst) {
                        Poll::Ready(if later == Saves::FailLater {
                            failed()
                        } else {
                            now(inner.save(journal.clone(), expected_revision))
                        })
                    } else {
                        // Polled again soon: the test releases it from another thread.
                        let waker = cx.waker().clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(1));
                            waker.wake();
                        });
                        Poll::Pending
                    }
                })
                .boxed()
            }
            Saves::Hang => futures::future::pending().boxed(),
        }
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>> {
        self.inner.delete_scope(scope)
    }
}

/// The fake, whose commits never answer while `hang` is set.
struct Operations {
    inner: Arc<FakeOperations>,
    hang: AtomicBool,
}

impl OperationService for Operations {
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
        if self.hang.load(Ordering::SeqCst) {
            return futures::future::pending().boxed();
        }
        self.inner.commit(transaction_id)
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
        self.inner.submit_record(record, origin, transaction_id)
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
    const KIND: &'static str = "history.failures.set";
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
            ["failures.value"],
            before,
            operation.0,
        ))
    }
}

struct Fixture {
    edits: Arc<dyn OperationService>,
    operations: Arc<Operations>,
    history: Arc<dyn HistoryService>,
    store: Arc<Store>,
    target: Arc<Target>,
    target_id: OperationTargetId,
    scope: HistoryScopeId,
    failures: Arc<Mutex<Vec<String>>>,
}

impl Fixture {
    fn new() -> Self {
        let inner = FakeOperations::new();
        let operations = Arc::new(Operations {
            inner: Arc::clone(&inner),
            hang: AtomicBool::new(false),
        });
        let store = Arc::new(Store {
            inner: InMemoryHistoryStore::default(),
            saves: Mutex::new(Saves::Succeed),
            released: Arc::default(),
        });
        let history = rayx_services_history::testing::history_with_timer(
            Arc::clone(&operations) as Arc<dyn OperationService>,
            Some(Arc::clone(&store) as Arc<dyn HistoryStore>),
            thread_timer(),
            TIMEOUT,
        );
        let failures = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&failures);
        std::mem::forget(history.subscribe(move |event| {
            if let HistoryEvent::NavigationFailed(failure) = event {
                sink.lock().unwrap().push(failure.error_code.clone());
            }
        }));
        let target = Arc::new(Target::default());
        let target_id = OperationTargetId::new("failures:target").unwrap();
        let scope = HistoryScopeId::new("failures:scope").unwrap();
        now(inner.register_target(
            target_id.clone(),
            OperationTargetHandle::of::<Set, _>(target_id.clone(), Arc::clone(&target)),
        ))
        .unwrap();
        block_on(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
        Self {
            edits: inner,
            operations,
            history,
            store,
            target,
            target_id,
            scope,
            failures,
        }
    }

    fn saves(&self, saves: Saves) {
        *self.store.saves.lock().unwrap() = saves;
    }

    fn set(&self, value: u64) {
        now(self
            .edits
            .submit(&self.target_id, Set(value), OperationOrigin::LocalUser))
        .unwrap();
    }

    fn value(&self) -> u64 {
        *self.target.0.lock().unwrap()
    }

    fn failures(&self) -> Vec<String> {
        self.failures.lock().unwrap().clone()
    }
}

#[test]
fn a_failed_save_behind_an_edit_is_a_failed_step_of_the_scope() {
    let f = Fixture::new();
    f.saves(Saves::Fail);
    f.set(1);
    assert_eq!(f.failures().len(), 1, "{:?}", f.failures());
    assert!(
        f.failures()[0].contains("the quota is full"),
        "{:?}",
        f.failures()
    );
    // The edit is kept, and saved once the store takes it.
    assert_eq!(f.history.stack_depths(&f.scope), (1, 0));
    f.saves(Saves::Succeed);
    f.set(2);
    let stored = now(f.store.inner.load(&f.scope)).unwrap().unwrap();
    assert_eq!(stored.entries.len(), 2);
}

#[test]
fn the_failure_of_an_undo_whose_caller_gave_up_is_a_failed_step_of_the_scope() {
    let f = Fixture::new();
    f.set(1);
    f.saves(Saves::FailLater);
    let mut undo = f.history.undo(f.scope.clone());
    assert!(poll_once(&mut undo).is_pending());
    drop(undo);
    f.store.released.store(true, Ordering::SeqCst);
    eventually("the failure is reported", || !f.failures().is_empty());
    assert!(
        f.failures()[0].contains("the quota is full"),
        "{:?}",
        f.failures()
    );
    assert_eq!((f.value(), f.history.stack_depths(&f.scope)), (0, (0, 1)));
}

#[test]
fn a_save_that_never_answers_times_out_and_frees_the_writer() {
    let f = Fixture::new();
    f.set(1);
    f.saves(Saves::Hang);
    let started = Instant::now();
    assert_eq!(
        block_on(f.history.undo(f.scope.clone())),
        Err(HistoryError::TimedOut)
    );
    assert!(started.elapsed() < WAIT);
    // The undo itself happened; the scope and the writer are free again.
    assert_eq!((f.value(), f.history.stack_depths(&f.scope)), (0, (0, 1)));
    f.saves(Saves::Succeed);
    assert!(block_on(f.history.redo(f.scope.clone())).unwrap());
    assert_eq!(f.value(), 1);
    let stored = now(f.store.inner.load(&f.scope)).unwrap().unwrap();
    assert_eq!(stored.entries.len(), 1);
}

#[test]
fn a_commit_that_never_answers_times_out_and_frees_the_scope() {
    let f = Fixture::new();
    f.set(1);
    f.operations.hang.store(true, Ordering::SeqCst);
    assert_eq!(
        block_on(f.history.undo(f.scope.clone())),
        Err(HistoryError::TimedOut)
    );
    // Not busy for good: the next command runs.
    f.operations.hang.store(false, Ordering::SeqCst);
    assert_ne!(
        block_on(f.history.clear_scope(f.scope.clone())),
        Err(HistoryError::ScopeBusy)
    );
    assert_eq!(f.history.stack_depths(&f.scope), (0, 0));
}

#[test]
fn a_lock_poisoned_while_a_save_is_pending_leaves_no_save_unanswered() {
    let store = Arc::new(Store {
        inner: InMemoryHistoryStore::default(),
        saves: Mutex::new(Saves::SucceedLater),
        released: Arc::default(),
    });
    let operations: Arc<dyn OperationService> = FakeOperations::new();
    let (history, poison) = rayx_services_history::testing::history_with_poison(
        Arc::clone(&operations),
        Some(Arc::clone(&store) as Arc<dyn HistoryStore>),
    );
    let first = HistoryScopeId::new("poison:first").unwrap();
    let second = HistoryScopeId::new("poison:second").unwrap();
    // The first clear's save waits in the writer; the second's waits behind it.
    let clears = [first, second].map(|scope| {
        let history = Arc::clone(&history);
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(block_on(history.clear_scope(scope)));
        });
        receive
    });
    std::thread::sleep(Duration::from_millis(20));
    poison();
    store.released.store(true, Ordering::SeqCst);
    for clear in clears {
        // Answered, whatever the answer (the state is poisoned).
        let _answer = clear
            .recv_timeout(WAIT)
            .expect("a save waiting behind the poisoned lock is answered");
    }
}
