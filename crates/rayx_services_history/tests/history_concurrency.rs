//! RayX's history on several threads at once: operations recorded, undone and redone in many
//! scopes concurrently; the published state ends as the scopes are, and the last state each
//! subscriber heard is the published one.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::FutureExt as _;
use rayx_app::services::history::{HistoryScopeId, HistoryScopeState, HistoryState};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::testing::FakeOperations;
use serde::{Deserialize, Serialize};

const THREADS: usize = 8;
const EDITS: usize = 24;

/// What `call` answered: the services here answer at once.
fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.concurrency.set";
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
        let mut value = self.0.lock().unwrap();
        let before = std::mem::replace(&mut *value, operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["value"],
            before,
            operation.0,
        ))
    }
}

#[test]
fn concurrent_scopes_end_in_the_published_state() {
    let operations = FakeOperations::new() as Arc<dyn OperationService>;
    let history = rayx_services_history::testing::history(Arc::clone(&operations));
    let last_heard: Arc<Mutex<Option<Arc<HistoryState>>>> = Arc::default();
    let _subscription = history.history().subscribe({
        let last_heard = Arc::clone(&last_heard);
        move |state| *last_heard.lock().unwrap() = Some(Arc::clone(state))
    });
    let barrier = Arc::new(Barrier::new(THREADS));
    let targets: Vec<(OperationTargetId, HistoryScopeId, Arc<Target>)> = (0..THREADS)
        .map(|thread| {
            let target = Arc::new(Target::default());
            let id = OperationTargetId::new(format!("target:{thread}")).unwrap();
            let scope = HistoryScopeId::new(format!("scope:{thread}")).unwrap();
            now(operations.register_handler(id.clone(), Arc::clone(&target))).unwrap();
            (id, scope, target)
        })
        .collect();
    // Edits on every thread at once. (A thread's operation event may be delivered by the thread
    // delivering another's: the history records it then, before that thread's submit returns.)
    let workers: Vec<_> = targets
        .iter()
        .cloned()
        .map(|(id, scope, _)| {
            let operations = Arc::clone(&operations);
            let history = Arc::clone(&history);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                now(history.bind_target_scope(id.clone(), scope)).unwrap();
                for value in 1..=EDITS as u64 {
                    now(operations.submit(&id, Set(value), OperationOrigin::LocalUser)).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    for (_, scope, _) in &targets {
        assert_eq!(history.stack_depths(scope), (EDITS, 0), "{scope:?}");
    }
    // Then undo and redo in every scope at once: back by a third, forward by one.
    let workers: Vec<_> = targets
        .iter()
        .cloned()
        .map(|(_, scope, target)| {
            let history = Arc::clone(&history);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..EDITS / 3 {
                    assert!(now(history.undo(scope.clone())).unwrap());
                }
                assert!(now(history.redo(scope.clone())).unwrap());
                *target.0.lock().unwrap()
            })
        })
        .collect();
    let values: Vec<u64> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    let published = history.history().get();
    for (thread, value) in values.into_iter().enumerate() {
        assert_eq!(value, (EDITS - EDITS / 3 + 1) as u64);
        let scope = HistoryScopeId::new(format!("scope:{thread}")).unwrap();
        let state: HistoryScopeState = published.scope(&scope);
        assert_eq!(
            (state.undo_depth, state.redo_depth),
            (EDITS - EDITS / 3 + 1, EDITS / 3 - 1),
            "{scope:?}"
        );
        assert_eq!(
            history.stack_depths(&scope),
            (state.undo_depth, state.redo_depth)
        );
    }
    let heard = last_heard
        .lock()
        .unwrap()
        .clone()
        .expect("a state was heard");
    assert_eq!(
        *heard, *published,
        "the last state heard is the published one"
    );
}
