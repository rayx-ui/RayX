//! RayX's operation service keeps the contract's rule on transaction ids: every id it gives
//! (`begin`, a submission outside a transaction, a remote operation) is unique and greater than
//! every id it gave before, from many threads at once.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::future::Future;
use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_services_operations::testing::operations;
use serde::{Deserialize, Serialize};

fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "ids.set";
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
            ["value"],
            before,
            operation.0 + 1,
        ))
    }
}

/// The ids `operations` gives one thread, in order: from `begin`, a submission and a remote
/// operation, `rounds` times.
fn ids(
    operations: &Arc<dyn OperationService>,
    target: &OperationTargetId,
    rounds: u64,
) -> Vec<u64> {
    let mut ids = Vec::new();
    for round in 0..rounds {
        ids.push(now(operations.begin()).unwrap());
        ids.push(
            now(operations.submit(target, Set(round), OperationOrigin::LocalUser))
                .unwrap()
                .transaction_id,
        );
        ids.push(
            now(operations.publish_remote(target, Set(round), "peer", 0, 1, ["value"]))
                .unwrap()
                .transaction_id,
        );
    }
    ids
}

#[test]
fn transaction_ids_are_unique_and_increasing_from_many_threads() {
    let operations: Arc<dyn OperationService> = operations();
    let target = OperationTargetId::new("ids").unwrap();
    now(operations.register_handler(target.clone(), Arc::new(Target::default()))).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let operations = Arc::clone(&operations);
            let target = target.clone();
            std::thread::spawn(move || ids(&operations, &target, 50))
        })
        .collect();
    let mut all = BTreeSet::new();
    for thread in threads {
        let ids = thread.join().unwrap();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
        for id in ids {
            assert!(all.insert(id), "{id} given twice");
        }
    }
    assert_eq!(all.len(), 8 * 50 * 3);
    let after = now(operations.begin()).unwrap();
    assert!(all.iter().all(|id| *id < after));
}
