//! The operation contract in the core, NSH-free: `rayx_app::testing::FakeOperations` registered
//! as `dyn OperationService`, the helpers of `dyn OperationService` over its members (typed
//! handlers and submissions, transactions entered on the thread in their three modes, a dropped
//! transaction aborting) and a failed call in the contract's error type.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::executor::block_on;
use rayx_app::services::ContractFailure;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationError, OperationEvent, OperationOrigin,
    OperationService, OperationTargetHandler, OperationTargetId, OperationTransactionMode,
};
use rayx_app::testing::FakeOperations;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Set(u32);

impl DomainOperation for Set {
    const KIND: &'static str = "contract.set";
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
        let mut value = self.0.lock().unwrap();
        let before = std::mem::replace(&mut *value, operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            u64::from(before),
            u64::from(operation.0),
        ))
    }
}

/// What `call` answered: the fake answers at once.
fn now<F: std::future::Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

/// The fake, resolved by trait, with the target `doc`.
fn operations() -> (Arc<dyn OperationService>, Arc<Target>, OperationTargetId) {
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn OperationService>(
            FakeOperations::new() as Arc<dyn OperationService>
        );
    let services = builder.build().unwrap();
    let operations = services.required_service::<dyn OperationService>().unwrap();
    let target = Arc::new(Target::default());
    let id = OperationTargetId::new("doc").unwrap();
    block_on(operations.register_handler(id.clone(), Arc::clone(&target))).unwrap();
    (operations, target, id)
}

#[test]
fn typed_submissions_cross_as_records() {
    let (operations, target, id) = operations();
    assert_eq!(operations.targets(), std::slice::from_ref(&id));
    let receipt = block_on(operations.submit(&id, Set(3), OperationOrigin::LocalUser)).unwrap();
    assert_eq!(*target.0.lock().unwrap(), 3);
    assert_eq!(receipt.forward.decode::<Set>().unwrap(), Set(3));
    assert_eq!(receipt.inverse.unwrap().decode::<Set>().unwrap(), Set(0));
    // A duplicate target is refused.
    assert_eq!(
        block_on(operations.register_handler(id.clone(), Arc::new(Target::default()))),
        Err(OperationError::DuplicateTarget("doc".into()))
    );
}

#[test]
fn transactions_on_the_thread_join_nest_or_refuse() {
    let (operations, target, id) = operations();
    let events = Arc::new(Mutex::new(Vec::new()));
    let _subscription = operations.subscribe({
        let events = Arc::clone(&events);
        move |event| {
            if let OperationEvent::Committed(receipt) = event {
                events.lock().unwrap().push(receipt.transaction_id);
            }
        }
    });
    block_on(async {
        assert_eq!(operations.current_transaction_id(), None);
        let outer = operations
            .begin_transaction(OperationTransactionMode::New)
            .await
            .unwrap();
        // Begun, it is carried, not entered on the thread.
        assert_eq!(operations.current_transaction_id(), None);
        let inner = outer.enter(|| {
            assert_eq!(operations.current_transaction_id(), Some(outer.id()));
            now(operations.submit(&id, Set(1), OperationOrigin::LocalUser)).unwrap();
            let joined = now(operations.begin_transaction(OperationTransactionMode::Join)).unwrap();
            assert!(joined.is_joined());
            assert_eq!(joined.id(), outer.id());
            now(joined.commit()).unwrap();
            assert!(matches!(
                now(operations.begin_transaction(OperationTransactionMode::Reject)),
                Err(OperationError::NestedTransactionRejected)
            ));
            operations.begin_transaction(OperationTransactionMode::New)
        });
        assert_eq!(operations.current_transaction_id(), None);
        let inner = inner.await.unwrap();
        assert_ne!(inner.id(), outer.id());
        inner
            .submit(&id, Set(2), OperationOrigin::LocalUser)
            .await
            .unwrap();
        // Dropped: aborted, its operation undone.
        drop(inner);
        assert_eq!(*target.0.lock().unwrap(), 1);
        assert!(events.lock().unwrap().is_empty());
        let outer_id = outer.id();
        outer.commit().await.unwrap();
        assert_eq!(*events.lock().unwrap(), [outer_id]);
        assert_eq!(operations.current_transaction_id(), None);
    });
}

#[test]
fn a_transaction_belongs_to_its_thread() {
    let (operations, target, id) = operations();
    block_on(async {
        let transaction = operations
            .begin_transaction(OperationTransactionMode::New)
            .await
            .unwrap();
        transaction.enter(|| {
            let elsewhere = Arc::clone(&operations);
            let other = std::thread::spawn(move || elsewhere.current_transaction_id())
                .join()
                .unwrap();
            assert_eq!(other, None);
            now(operations.submit(&id, Set(5), OperationOrigin::LocalUser)).unwrap();
        });
        transaction.abort().await.unwrap();
    });
    assert_eq!(*target.0.lock().unwrap(), 0);
}

#[test]
fn a_failed_call_is_the_contract_s_error() {
    let error = OperationError::from(ContractFailure {
        contract: "OperationService",
        member: "submit_record",
        message: "the engine is gone".into(),
    });
    assert_eq!(error.code(), "operation_call_failed");
    assert!(error.to_string().contains("the engine is gone"), "{error}");
}
