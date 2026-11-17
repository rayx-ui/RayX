//! RayX's operation service on several threads at once: targets registered and released, and
//! operations submitted, concurrently; the published state ends as the targets are, and the
//! last state each subscriber heard is the published one.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::FutureExt as _;
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationEvent, OperationOrigin, OperationState,
    OperationTargetHandler, OperationTargetId,
};
use rayx_services_operations::testing::operations;
use serde::{Deserialize, Serialize};

const THREADS: usize = 8;
const TARGETS: usize = 16;

#[derive(Clone, Deserialize, Serialize)]
struct Add(u64);

impl DomainOperation for Add {
    const KIND: &'static str = "test.concurrency.add";
    const VERSION: u32 = 1;
}

#[derive(Default)]
struct Counter(Mutex<u64>);

impl OperationTargetHandler<Add> for Counter {
    fn apply_atomically(
        &self,
        operation: &Add,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Add>, String> {
        let mut value = self.0.lock().unwrap();
        let before = *value;
        *value += operation.0;
        Ok(OperationApplyResult::applied(
            Add(0),
            ["count"],
            before,
            *value,
        ))
    }
}

#[test]
fn concurrent_registrations_end_in_the_published_state() {
    let service = operations();
    let last_heard: Arc<Mutex<Option<Arc<OperationState>>>> = Arc::default();
    let _subscription = service.operations().subscribe({
        let last_heard = Arc::clone(&last_heard);
        move |state| *last_heard.lock().unwrap() = Some(Arc::clone(state))
    });
    let shared = Arc::new(Counter::default());
    let committed = Arc::new(Mutex::new(0usize));
    let _events = service.subscribe({
        let committed = Arc::clone(&committed);
        move |event| {
            if let OperationEvent::Committed(_) = event {
                *committed.lock().unwrap() += 1;
            }
        }
    });
    let barrier = Arc::new(Barrier::new(THREADS));
    let workers: Vec<_> = (0..THREADS)
        .map(|thread| {
            let service = Arc::clone(&service);
            let shared = Arc::clone(&shared);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for target in 0..TARGETS {
                    let id = OperationTargetId::new(format!("t{thread}:{target}")).unwrap();
                    service
                        .register_handler(id.clone(), Arc::clone(&shared))
                        .now_or_never()
                        .unwrap()
                        .unwrap();
                    service
                        .submit(&id, Add(1), OperationOrigin::LocalUser)
                        .now_or_never()
                        .unwrap()
                        .unwrap();
                    // Every other target goes again.
                    if target % 2 == 1 {
                        assert!(service.release_target(id).now_or_never().unwrap().unwrap());
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let published = service.operations().get();
    assert_eq!(published.targets.len(), THREADS * TARGETS / 2);
    assert!(published.targets.windows(2).all(|pair| pair[0] < pair[1]));
    let heard = last_heard
        .lock()
        .unwrap()
        .clone()
        .expect("a state was heard");
    assert_eq!(
        *heard, *published,
        "the last state heard is the published one"
    );
    assert_eq!(*shared.0.lock().unwrap(), (THREADS * TARGETS) as u64);
    assert_eq!(*committed.lock().unwrap(), THREADS * TARGETS);
}
