//! RayX's history over RayX's operation service, end to end: `add_operations().add_history()`
//! records the local user's edits from the service's receipts, undoes and redoes them through
//! it (outside a transaction and inside one, a transaction being one entry), records nothing
//! of an aborted transaction, and, with the `storage` feature, keeps its journal in RayX's
//! in-memory key-value store.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx::app::di::StartupInitializationContext;
use rayx::app::services::history::{HistoryEvent, HistoryScopeId, HistoryService};
use rayx::app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId, OperationTransactionMode,
};
use rayx::app::{RayXAppBuilder, ServiceProvider};
use rayx::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "umbrella.history.set";
    const VERSION: u32 = 1;
}

#[derive(Default)]
struct Document(Mutex<u64>);

impl OperationTargetHandler<Set> for Document {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let before = std::mem::replace(&mut *self.0.lock().unwrap(), operation.0);
        Ok(OperationApplyResult::applied(
            Set(before),
            ["document.value"],
            before,
            operation.0,
        ))
    }
}

/// An app with RayX's operations and history, started, its document bound to a scope.
struct App {
    services: ServiceProvider,
    operations: Arc<dyn OperationService>,
    history: Arc<dyn HistoryService>,
    document: Arc<Document>,
    target: OperationTargetId,
    scope: HistoryScopeId,
}

impl App {
    fn start(builder: RayXAppBuilder) -> Self {
        let services = builder.build().unwrap();
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .unwrap();
        let operations = services.required_service::<dyn OperationService>().unwrap();
        let history = services.required_service::<dyn HistoryService>().unwrap();
        let document = Arc::new(Document::default());
        let target = OperationTargetId::new("document").unwrap();
        let scope = HistoryScopeId::new("editor").unwrap();
        block_on(operations.register_handler(target.clone(), Arc::clone(&document))).unwrap();
        block_on(history.bind_target_scope(target.clone(), scope.clone())).unwrap();
        Self {
            services,
            operations,
            history,
            document,
            target,
            scope,
        }
    }

    fn set(&self, value: u64) -> u64 {
        block_on(
            self.operations
                .submit(&self.target, Set(value), OperationOrigin::LocalUser),
        )
        .unwrap()
        .transaction_id
    }

    fn value(&self) -> u64 {
        *self.document.0.lock().unwrap()
    }

    fn depths(&self) -> (usize, usize) {
        self.history.stack_depths(&self.scope)
    }

    fn undo(&self) -> bool {
        block_on(self.history.undo(self.scope.clone())).unwrap()
    }

    fn redo(&self) -> bool {
        block_on(self.history.redo(self.scope.clone())).unwrap()
    }
}

fn in_memory() -> App {
    App::start(RayXAppBuilder::new().add_operations().add_history())
}

#[test]
fn edits_are_undone_and_redone_through_rayx_s_operation_service() {
    let app = in_memory();
    let steps = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&steps);
    let _subscription = app.history.subscribe(move |event| {
        sink.lock().unwrap().push(match event {
            HistoryEvent::Recorded(entry) => format!("recorded {}", entry.transaction_id),
            HistoryEvent::UndoCompleted(_) => "undone".to_owned(),
            HistoryEvent::RedoCompleted(_) => "redone".to_owned(),
            other => format!("{other:?}"),
        });
    });
    let first = app.set(1);
    let second = app.set(2);
    // Each submission outside a transaction is a transaction of its own: an entry each.
    assert_ne!(first, second);
    assert_eq!(app.depths(), (2, 0));
    assert!(app.undo());
    assert_eq!((app.value(), app.depths()), (1, (1, 1)));
    assert!(app.undo());
    assert_eq!((app.value(), app.depths()), (0, (0, 2)));
    assert!(!app.undo());
    assert!(app.redo());
    assert!(app.redo());
    assert_eq!((app.value(), app.depths()), (2, (2, 0)));
    assert_eq!(
        *steps.lock().unwrap(),
        [
            format!("recorded {first}"),
            format!("recorded {second}"),
            "undone".to_owned(),
            "undone".to_owned(),
            "redone".to_owned(),
            "redone".to_owned(),
        ]
    );
    // The replays are not recorded as edits of their own.
    assert_eq!(app.depths(), (2, 0));
}

#[test]
fn a_transaction_is_one_entry_and_an_aborted_one_none() {
    let app = in_memory();
    app.set(1);
    let transaction = block_on(
        app.operations
            .begin_transaction(OperationTransactionMode::New),
    )
    .unwrap();
    block_on(transaction.submit(&app.target, Set(2), OperationOrigin::LocalUser)).unwrap();
    // Entered for a synchronous body, the helpers submit in it too.
    transaction.enter(|| app.set(3));
    // Recorded when it commits.
    assert_eq!(app.depths(), (1, 0));
    block_on(transaction.commit()).unwrap();
    assert_eq!((app.value(), app.depths()), (3, (2, 0)));

    let aborted = block_on(
        app.operations
            .begin_transaction(OperationTransactionMode::New),
    )
    .unwrap();
    block_on(aborted.submit(&app.target, Set(9), OperationOrigin::LocalUser)).unwrap();
    block_on(aborted.abort()).unwrap();
    assert_eq!((app.value(), app.depths()), (3, (2, 0)));

    // The transaction undoes and redoes as one.
    assert!(app.undo());
    assert_eq!((app.value(), app.depths()), (1, (1, 1)));
    assert!(app.redo());
    assert_eq!((app.value(), app.depths()), (3, (2, 0)));
}

#[test]
fn each_app_s_operation_service_is_an_instance_of_its_own() {
    let first = in_memory();
    let second = in_memory();
    let id = first.operations.instance_id();
    assert_eq!(first.operations.instance_id(), id);
    assert_ne!(second.operations.instance_id(), id);
    // A transaction entered for one service is not the other's.
    let transaction = block_on(
        first
            .operations
            .begin_transaction(OperationTransactionMode::New),
    )
    .unwrap();
    transaction.enter(|| {
        assert_eq!(
            first.operations.current_transaction_id(),
            Some(transaction.id())
        );
        assert_eq!(second.operations.current_transaction_id(), None);
        second.set(4);
    });
    block_on(transaction.commit()).unwrap();
    assert_eq!((second.value(), second.depths()), (4, (1, 0)));
    assert_eq!(first.depths(), (0, 0));
    drop(first.services);
}

#[cfg(feature = "storage")]
#[test]
fn persistent_history_keeps_its_journal_in_rayx_s_storage() {
    use rayx::app::services::storage::KeyValueStore;
    use rayx::history::{HISTORY_BUCKET, HistoryJournalV2};
    use rayx_services_storage::StorageAppBuilderExt as _;

    let app = App::start(
        RayXAppBuilder::new()
            .add_in_memory_storage()
            .add_operations()
            .add_persistent_history(),
    );
    let values = app
        .services
        .required_service::<dyn KeyValueStore>()
        .unwrap();
    let journal = || {
        block_on(values.get(HISTORY_BUCKET.into(), app.scope.as_str().into()))
            .unwrap()
            .map(|bytes| HistoryJournalV2::decode(&bytes).unwrap().entries.len())
    };
    app.set(1);
    app.set(2);
    assert_eq!(journal(), Some(2));
    assert!(app.undo());
    assert_eq!((app.value(), journal()), (1, Some(1)));
    assert!(app.redo());
    assert_eq!((app.value(), journal()), (2, Some(2)));
}
