//! The history contract in the core, NSH-free: `rayx_app::testing::FakeHistory` registered as
//! `dyn HistoryService`, the lookups of `dyn HistoryService` over its published state, its
//! events through `subscribe` (a subscriber that panics stops no other), and a failed call in
//! the contract's error type.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::services::ContractFailure;
use rayx_app::services::history::{HistoryError, HistoryEvent, HistoryScopeId, HistoryService};
use rayx_app::services::operations::OperationTargetId;
use rayx_app::testing::FakeHistory;
use rayx_app::{AppBuilder as _, RayXAppBuilder};

#[test]
fn the_fake_is_a_history_resolved_by_trait() {
    let fake = FakeHistory::new();
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn HistoryService>(Arc::clone(&fake) as Arc<dyn HistoryService>);
    let services = builder.build().unwrap();
    let history = services.required_service::<dyn HistoryService>().unwrap();
    let scope = HistoryScopeId::new("document").unwrap();
    assert!(!history.can_undo(&scope), "an unknown scope is empty");

    let _panics = history.subscribe(|_| panic!("a subscriber that fails"));
    let heard = Arc::new(Mutex::new(Vec::new()));
    let _heard = history.subscribe({
        let heard = Arc::clone(&heard);
        move |event| heard.lock().unwrap().push(event.clone())
    });

    fake.set_depths(&scope, 2, 0);
    assert!(history.can_undo(&scope));
    assert!(!history.can_redo(&scope));
    assert!(block_on(history.undo(scope.clone())).unwrap());
    assert_eq!(history.stack_depths(&scope), (1, 1));
    let diagnostics = history.diagnostics(&scope);
    assert_eq!((diagnostics.undo_depth, diagnostics.redo_depth), (1, 1));
    assert_eq!(diagnostics.stack_revision, 2);
    assert_eq!(diagnostics.subscriber_count, 2);
    assert_eq!(
        *heard.lock().unwrap(),
        [HistoryEvent::UndoCompleted(scope.clone())]
    );

    let target = OperationTargetId::new("doc").unwrap();
    block_on(history.bind_target_scope(target.clone(), scope.clone())).unwrap();
    assert_eq!(fake.bindings(), [(target, scope.clone())]);
    assert_eq!(
        block_on(history.set_scope_entry_limit(scope, 0)),
        Err(HistoryError::InvalidEntryLimit)
    );
}

#[test]
fn a_failed_call_is_the_contract_s_error() {
    let error = HistoryError::from(ContractFailure {
        contract: "HistoryService",
        member: "undo",
        message: "the engine is gone".into(),
    });
    assert!(error.to_string().contains("the engine is gone"), "{error}");
    assert_eq!(
        HistoryScopeId::new(" ").unwrap_err(),
        HistoryError::InvalidScopeId
    );
}
