//! Transactions carried explicitly: a transaction held across an `.await` is never another
//! task's on the same thread, one begun on a thread may close on another, a submission names an
//! open transaction or fails, and a target's handle that crossed as JSON lives only as long as
//! the handle that crossed, unless it is taken.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};

use futures::executor::block_on;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationError, OperationEvent, OperationOrigin,
    OperationRecord, OperationService, OperationTargetHandle, OperationTargetHandler,
    OperationTargetId, OperationTransactionMode,
};
use rayx_app::testing::FakeOperations;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "transactions.set";
    const VERSION: u32 = 1;
}

#[derive(Default)]
struct Target(Mutex<u32>);

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let before = std::mem::replace(&mut *self.0.lock().unwrap(), operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            u64::from(before),
            u64::from(operation.0),
        ))
    }
}

/// Lets the other tasks of the thread run once.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// The fake with the targets `a` and `b`, and the transaction ids of the committed events.
#[allow(clippy::type_complexity)]
fn operations() -> (
    Arc<dyn OperationService>,
    [(Arc<Target>, OperationTargetId); 2],
    Arc<Mutex<Vec<u64>>>,
) {
    let operations: Arc<dyn OperationService> = FakeOperations::new();
    let targets = ["a", "b"].map(|name| {
        let target = Arc::new(Target::default());
        let id = OperationTargetId::new(name).unwrap();
        block_on(operations.register_handler(id.clone(), Arc::clone(&target))).unwrap();
        (target, id)
    });
    let committed = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&committed);
    // Kept for the operation service's life.
    std::mem::forget(operations.subscribe(move |event| {
        if let OperationEvent::Committed(receipt) = event {
            sink.lock().unwrap().push(receipt.transaction_id);
        }
    }));
    (operations, targets, committed)
}

#[test]
fn a_transaction_held_across_an_await_is_not_another_task_s() {
    let (operations, [(a, a_id), (b, b_id)], committed) = operations();
    let first = async {
        let transaction = operations
            .begin_transaction(OperationTransactionMode::New)
            .await
            .unwrap();
        transaction
            .submit(&a_id, Set(1), OperationOrigin::LocalUser)
            .await
            .unwrap();
        // The other task runs while this one holds its transaction.
        YieldNow(false).await;
        transaction.abort().await.unwrap();
    };
    let second = async {
        assert_eq!(operations.current_transaction_id(), None);
        operations
            .submit(&b_id, Set(7), OperationOrigin::LocalUser)
            .await
            .unwrap()
    };
    let ((), receipt) = block_on(futures::future::join(first, second));
    // The other task's operation committed alone and survived the abort.
    assert_eq!(*committed.lock().unwrap(), [receipt.transaction_id]);
    assert_eq!((*a.0.lock().unwrap(), *b.0.lock().unwrap()), (0, 7));
}

#[test]
fn a_transaction_begun_on_one_thread_commits_on_another() {
    let (operations, [(a, a_id), _], committed) = operations();
    let transaction =
        block_on(operations.begin_transaction(OperationTransactionMode::New)).unwrap();
    let id = transaction.id();
    block_on(transaction.submit(&a_id, Set(1), OperationOrigin::LocalUser)).unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(move || block_on(transaction.commit()).unwrap())
            .join()
            .unwrap();
    });
    assert_eq!(*committed.lock().unwrap(), [id]);
    // Nothing stays open on the first thread: later submissions are each their own.
    assert_eq!(operations.current_transaction_id(), None);
    let second = block_on(operations.submit(&a_id, Set(2), OperationOrigin::LocalUser)).unwrap();
    let third = block_on(operations.submit(&a_id, Set(3), OperationOrigin::LocalUser)).unwrap();
    assert_ne!(second.transaction_id, third.transaction_id);
    assert_ne!(second.transaction_id, id);
    assert_eq!(committed.lock().unwrap().len(), 3);
    assert_eq!(*a.0.lock().unwrap(), 3);
}

#[test]
fn a_submission_into_a_transaction_that_is_not_open_fails() {
    let (operations, [(a, a_id), _], committed) = operations();
    let record = OperationRecord::from_operation(a_id.clone(), &Set(4)).unwrap();
    // Never begun.
    assert_eq!(
        block_on(operations.submit_record(record.clone(), OperationOrigin::LocalUser, Some(99))),
        Err(OperationError::TransactionClosed)
    );
    // Committed already.
    let transaction =
        block_on(operations.begin_transaction(OperationTransactionMode::New)).unwrap();
    let id = transaction.id();
    block_on(transaction.commit()).unwrap();
    assert_eq!(
        block_on(operations.submit_record(record, OperationOrigin::LocalUser, Some(id))),
        Err(OperationError::TransactionClosed)
    );
    assert_eq!(*a.0.lock().unwrap(), 0);
    assert!(committed.lock().unwrap().is_empty());
}

/// The handle of a new target, and the target, weakly.
fn handle() -> (OperationTargetHandle, Weak<Target>) {
    let target = Arc::new(Target::default());
    let weak = Arc::downgrade(&target);
    let handle =
        OperationTargetHandle::of::<Set, _>(OperationTargetId::new("crossing").unwrap(), target);
    (handle, weak)
}

fn number(json: &serde_json::Value) -> u64 {
    json["handle"].as_u64().unwrap()
}

#[test]
fn a_handle_nobody_took_is_released_with_the_handle_that_crossed() {
    let (handle, target) = handle();
    let crossed = serde_json::to_value(&handle).unwrap();
    let kept = handle.clone();
    drop(handle);
    // A clone keeps it.
    assert!(target.upgrade().is_some());
    drop(kept);
    assert!(target.upgrade().is_none(), "the handler was leaked");
    assert!(OperationTargetHandle::take(number(&crossed)).is_none());
}

#[test]
fn a_handle_taken_lives_with_whoever_took_it() {
    let (handle, target) = handle();
    let crossed = serde_json::to_value(&handle).unwrap();
    let taken = OperationTargetHandle::take(number(&crossed)).unwrap();
    drop(handle);
    assert!(target.upgrade().is_some());
    drop(taken);
    assert!(target.upgrade().is_none());
}

#[test]
fn the_fake_s_transaction_ids_are_unique_and_increasing() {
    let (operations, [(_, a_id), _], _) = operations();
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let operations = Arc::clone(&operations);
            let a_id = a_id.clone();
            std::thread::spawn(move || {
                let mut ids = Vec::new();
                for value in 0..50 {
                    ids.push(block_on(operations.begin()).unwrap());
                    ids.push(
                        block_on(operations.submit(&a_id, Set(value), OperationOrigin::LocalUser))
                            .unwrap()
                            .transaction_id,
                    );
                }
                ids
            })
        })
        .collect();
    let mut all = std::collections::BTreeSet::new();
    for thread in threads {
        let ids = thread.join().unwrap();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
        assert!(ids.into_iter().all(|id| all.insert(id)));
    }
    let after = block_on(operations.begin()).unwrap();
    assert!(all.iter().all(|id| *id < after));
}
