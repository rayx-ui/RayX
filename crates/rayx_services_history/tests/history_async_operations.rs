//! RayX's history over an operation service that answers later (as a TS implementation does):
//! its replay carries its transaction by id, so another task on the thread neither joins it nor
//! finds the history busy, a replay finished on another thread leaves nothing open behind; an
//! edit recorded while an undo is in flight is kept, and a dropped undo still completes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationError, OperationEvent, OperationOrigin,
    OperationReceipt, OperationRecord, OperationService, OperationState, OperationTargetHandle,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::testing::FakeOperations;
use rayx_app::{EventStream, StateCell};
use serde::{Deserialize, Serialize};

/// What `call` answered: the call answers at once.
fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

/// Holds submissions until it is opened.
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
}

/// `rayx_app`'s fake, whose `submit_record` waits at the gate (as a TS implementation's waits
/// for JS's next turn); the rest answers at once.
struct Delayed {
    inner: Arc<FakeOperations>,
    gate: Arc<Gate>,
}

impl OperationService for Delayed {
    fn operations(&self) -> &StateCell<OperationState> {
        self.inner.operations()
    }

    fn events(&self) -> &EventStream<OperationEvent> {
        self.inner.events()
    }

    fn instance_id(&self) -> u64 {
        // The fake's: a transaction entered for one is the other's.
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
        async move {
            self.gate.pass().await;
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
    const KIND: &'static str = "history.async.set";
    const VERSION: u32 = 1;
}

/// A value; its replays fail while `fail` is set.
#[derive(Default)]
struct Target {
    value: Mutex<u64>,
    fail: AtomicBool,
}

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        if *origin != OperationOrigin::LocalUser && self.fail.load(Ordering::SeqCst) {
            return Err("injected replay failure".into());
        }
        let before = std::mem::replace(&mut *self.value.lock().unwrap(), operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["async.value"],
            before,
            operation.0,
        ))
    }
}

/// A scope of the history, its target and the target's id.
struct Scope {
    id: HistoryScopeId,
    target: Arc<Target>,
    target_id: OperationTargetId,
}

impl Scope {
    fn value(&self) -> u64 {
        *self.target.value.lock().unwrap()
    }
}

struct Fixture {
    /// The fake, answering at once: the app's own edits.
    operations: Arc<dyn OperationService>,
    history: Arc<dyn HistoryService>,
    gate: Arc<Gate>,
    a: Scope,
    b: Scope,
}

impl Fixture {
    fn new() -> Self {
        let inner = FakeOperations::new();
        let gate = Arc::new(Gate::default());
        let delayed: Arc<dyn OperationService> = Arc::new(Delayed {
            inner: Arc::clone(&inner),
            gate: Arc::clone(&gate),
        });
        let history = rayx_services_history::testing::history(Arc::clone(&delayed));
        let scope = |name: &str| {
            let target = Arc::new(Target::default());
            let target_id = OperationTargetId::new(format!("async:{name}")).unwrap();
            let id = HistoryScopeId::new(format!("async:{name}-scope")).unwrap();
            now(delayed.register_handler(target_id.clone(), Arc::clone(&target))).unwrap();
            now(history.bind_target_scope(target_id.clone(), id.clone())).unwrap();
            Scope {
                id,
                target,
                target_id,
            }
        };
        let (a, b) = (scope("a"), scope("b"));
        Self {
            operations: inner,
            history,
            gate,
            a,
            b,
        }
    }

    /// The app's edit of `scope`'s target, answered at once.
    fn set(&self, scope: &Scope, value: u64) -> OperationReceipt {
        now(self
            .operations
            .submit(&scope.target_id, Set(value), OperationOrigin::LocalUser))
        .unwrap()
    }
}

/// Polls `future` once, waking nobody.
fn poll_once<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    future.poll_unpin(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn another_task_on_the_thread_neither_joins_a_replay_nor_finds_its_scope_busy() {
    let f = Fixture::new();
    f.set(&f.a, 5);
    f.a.target.fail.store(true, Ordering::SeqCst);
    f.gate.hold();
    let mut undo = f.history.undo(f.a.id.clone());
    assert!(poll_once(&mut undo).is_pending());

    // Meanwhile, on the same thread: another scope is cleared and edited.
    now(f.history.clear_scope(f.b.id.clone())).unwrap();
    let edit = f.set(&f.b, 7);
    assert_eq!(f.history.stack_depths(&f.b.id), (1, 0));

    f.gate.open();
    assert!(block_on(undo).is_err());
    // The failed undo aborted its own transaction only.
    assert_eq!((f.a.value(), f.b.value()), (5, 7));
    assert_eq!(f.history.stack_depths(&f.a.id), (1, 0));
    assert_eq!(f.history.stack_depths(&f.b.id), (1, 0));
    assert!(block_on(f.history.undo(f.b.id.clone())).unwrap());
    assert_eq!(f.b.value(), 0);
    assert_ne!(edit.transaction_id, 0);

    // Its scope is free again.
    f.a.target.fail.store(false, Ordering::SeqCst);
    assert!(block_on(f.history.undo(f.a.id.clone())).unwrap());
    assert_eq!(f.a.value(), 0);
}

#[test]
fn a_replay_finished_on_another_thread_leaves_nothing_open_behind() {
    let f = Fixture::new();
    f.set(&f.a, 5);
    f.gate.hold();
    let mut undo = f.history.undo(f.a.id.clone());
    assert!(poll_once(&mut undo).is_pending());
    let gate = Arc::clone(&f.gate);
    let undone = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                gate.open();
                block_on(undo)
            })
            .join()
            .unwrap()
    });
    assert!(undone.unwrap());
    assert_eq!(f.a.value(), 0);

    // On the first thread, later edits are each an entry of their own, and the scope is free.
    assert_eq!(f.operations.current_transaction_id(), None);
    let first = f.set(&f.a, 1);
    let second = f.set(&f.a, 2);
    assert_ne!(first.transaction_id, second.transaction_id);
    assert_eq!(f.history.stack_depths(&f.a.id), (2, 0));
    now(f.history.clear_scope(f.a.id.clone())).unwrap();
    assert_eq!(f.history.stack_depths(&f.a.id), (0, 0));
}

#[test]
fn an_edit_recorded_while_an_undo_is_in_flight_is_kept() {
    let f = Fixture::new();
    f.set(&f.a, 5);
    f.gate.hold();
    let mut undo = f.history.undo(f.a.id.clone());
    assert!(poll_once(&mut undo).is_pending());
    f.set(&f.a, 9);
    assert_eq!(f.history.stack_depths(&f.a.id), (2, 0));
    f.gate.open();
    assert!(block_on(undo).unwrap());
    // The undo replayed the entry of 5 (back to 0) and moved that entry; the edit of 9 is the
    // one left to undo, back to what it replaced.
    assert_eq!(f.a.value(), 0);
    assert_eq!(f.history.stack_depths(&f.a.id), (1, 1));
    assert!(block_on(f.history.undo(f.a.id.clone())).unwrap());
    assert_eq!(f.a.value(), 5);
}

#[test]
fn a_dropped_undo_still_completes_and_frees_its_scope() {
    let f = Fixture::new();
    f.set(&f.a, 5);
    f.gate.hold();
    let mut undo = f.history.undo(f.a.id.clone());
    assert!(poll_once(&mut undo).is_pending());
    // Giving up on the undo gives up only the wait: started, it finishes.
    drop(undo);
    f.gate.open();
    assert_eq!(f.a.value(), 0);
    assert_eq!(f.history.stack_depths(&f.a.id), (0, 1));
    assert!(!block_on(f.history.undo(f.a.id.clone())).unwrap());
    assert!(block_on(f.history.redo(f.a.id.clone())).unwrap());
    assert_eq!(f.a.value(), 5);
    now(f.history.clear_scope(f.a.id.clone())).unwrap();
}
