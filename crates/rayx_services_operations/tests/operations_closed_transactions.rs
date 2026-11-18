//! RayX's operation service refuses a submission into a transaction that is not open (never
//! begun, committed or aborted), applying nothing and publishing no receipt, and undoes one
//! whose transaction closed while it applied.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationError, OperationEvent, OperationOrigin,
    OperationRecord, OperationService, OperationTargetHandler, OperationTargetId,
    OperationTransactionMode,
};
use rayx_services_operations::testing::operations;
use serde::{Deserialize, Serialize};

/// What `call` answered: RayX's operation service answers at once.
fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "closed.set";
    const VERSION: u32 = 1;
}

/// A target that runs `during` (once) while it applies a record.
#[derive(Default)]
struct Target {
    value: Mutex<u32>,
    during: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let before = std::mem::replace(&mut *self.value.lock().unwrap(), operation.0);
        let during = self.during.lock().unwrap().take();
        if let Some(during) = during {
            during();
        }
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            u64::from(before),
            u64::from(operation.0),
        ))
    }
}

#[allow(clippy::type_complexity)]
fn service() -> (
    Arc<dyn OperationService>,
    Arc<Target>,
    OperationTargetId,
    Arc<Mutex<Vec<OperationEvent>>>,
) {
    let service = operations();
    let target = Arc::new(Target::default());
    let id = OperationTargetId::new("closed").unwrap();
    now(service.register_handler(id.clone(), Arc::clone(&target))).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    std::mem::forget(service.subscribe(move |event| sink.lock().unwrap().push(event.clone())));
    (service, target, id, events)
}

#[test]
fn a_submission_into_a_closed_or_unknown_transaction_fails() {
    let (service, target, id, events) = service();
    let record = OperationRecord::from_operation(id.clone(), &Set(4)).unwrap();
    assert_eq!(
        now(service.submit_record(record.clone(), OperationOrigin::LocalUser, Some(1_000))),
        Err(OperationError::TransactionClosed)
    );
    for commit in [true, false] {
        let transaction = now(service.begin_transaction(OperationTransactionMode::New)).unwrap();
        let closed = transaction.id();
        if commit {
            now(transaction.commit()).unwrap();
        } else {
            now(transaction.abort()).unwrap();
        }
        assert_eq!(
            now(service.submit_record(record.clone(), OperationOrigin::LocalUser, Some(closed))),
            Err(OperationError::TransactionClosed)
        );
    }
    assert_eq!(*target.value.lock().unwrap(), 0);
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn a_submission_whose_transaction_aborts_while_it_applies_is_undone() {
    let (service, target, id, events) = service();
    let transaction = now(service.begin_transaction(OperationTransactionMode::New)).unwrap();
    let transaction_id = transaction.id();
    let aborting = Arc::clone(&service);
    *target.during.lock().unwrap() = Some(Box::new(move || {
        now(aborting.abort(transaction_id)).unwrap();
    }));
    let record = OperationRecord::from_operation(id, &Set(4)).unwrap();
    assert_eq!(
        now(service.submit_record(record, OperationOrigin::LocalUser, Some(transaction_id))),
        Err(OperationError::TransactionClosed)
    );
    assert_eq!(*target.value.lock().unwrap(), 0);
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .all(|event| matches!(event, OperationEvent::Failed(_)))
    );
    // Closed already: its drop has nothing to abort.
    drop(transaction);
}
