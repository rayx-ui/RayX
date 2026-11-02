//! History kept in the app's key-value store (`add_persistent_history`): a second app over the
//! same store undoes what the first one did, and the store's journals are revision-checked.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_core::di::StartupInitializationContext;
use rayx_core::{RayXAppBuilder, ServiceProvider};
use rayx_runtime::services::history::{
    HISTORY_BUCKET, HistoryAppBuilderExt, HistoryJournalV2, HistoryScopeId, HistoryService,
    HistoryStore, HistoryStoreError, KeyValueHistoryStore,
};
use rayx_runtime::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_runtime::services::storage::{InMemoryKeyValueStore, KeyValueStore, StorageAppBuilderExt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.persistence.set";
    const VERSION: u32 = 1;
}

/// A value an app restores on its own (as a document it saved), here from the start.
struct Target(Mutex<u64>);

impl OperationTargetHandler<Set> for Target {
    fn apply_atomically(
        &self,
        operation: &Set,
        _origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<Set>, String> {
        let mut value = self.0.lock().unwrap();
        let before = *value;
        *value = operation.0;
        Ok(OperationApplyResult::applied(
            Set(before),
            ["persistence.value"],
            before,
            operation.0,
        ))
    }
}

/// An app over `store`, started, with a target at `value` bound to the scope.
struct App {
    _services: ServiceProvider,
    operations: OperationService,
    history: HistoryService,
    target: Arc<Target>,
    target_id: OperationTargetId,
    scope: HistoryScopeId,
}

impl App {
    fn start(store: &InMemoryKeyValueStore, value: u64) -> Self {
        let services = RayXAppBuilder::new()
            .with_key_value_store(store.clone())
            .add_persistent_history()
            .build()
            .unwrap();
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .unwrap();
        let operations = services
            .required_service::<OperationService>()
            .unwrap()
            .as_ref()
            .clone();
        let history = services
            .required_service::<HistoryService>()
            .unwrap()
            .as_ref()
            .clone();
        let target = Arc::new(Target(Mutex::new(value)));
        let target_id = OperationTargetId::new("persistence:target").unwrap();
        let scope = HistoryScopeId::new("persistence:scope").unwrap();
        operations
            .register_target(target_id.clone(), target.clone())
            .unwrap();
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap();
        Self {
            _services: services,
            operations,
            history,
            target,
            target_id,
            scope,
        }
    }

    fn set(&self, value: u64) {
        self.operations
            .submit(&self.target_id, Set(value), OperationOrigin::LocalUser)
            .unwrap();
    }

    fn value(&self) -> u64 {
        *self.target.0.lock().unwrap()
    }
}

#[test]
fn a_second_app_over_the_same_store_undoes_the_first_app_s_edits() {
    let store = InMemoryKeyValueStore::new();
    let first = App::start(&store, 0);
    first.set(1);
    first.set(2);
    assert_eq!(first.history.stack_depths(&first.scope).unwrap(), (2, 0));
    assert!(
        store
            .get(HISTORY_BUCKET, first.scope.as_str())
            .unwrap()
            .is_some(),
        "the journal is in the app's store"
    );
    drop(first);

    // The app runs again (its document restored at 2): the history came back with the store.
    let second = App::start(&store, 2);
    assert_eq!(second.history.stack_depths(&second.scope).unwrap(), (2, 0));
    assert!(second.history.undo(&second.scope).unwrap());
    assert_eq!(second.value(), 1);
    assert!(second.history.undo(&second.scope).unwrap());
    assert_eq!(second.value(), 0);
    assert!(!second.history.undo(&second.scope).unwrap());
}

#[test]
fn the_store_checks_revisions_and_deletes_scopes() {
    let values: Arc<dyn KeyValueStore> = Arc::new(InMemoryKeyValueStore::new());
    let store = KeyValueHistoryStore::new(values.clone());
    let scope = HistoryScopeId::new("persistence:direct").unwrap();
    assert_eq!(store.load(&scope).unwrap(), None);

    let first = HistoryJournalV2::new(1, scope.clone(), Vec::new());
    store.save(first.clone(), None).unwrap();
    assert_eq!(store.load(&scope).unwrap(), Some(first));
    // A save that expects another revision than the store holds is refused.
    let second = HistoryJournalV2::new(2, scope.clone(), Vec::new());
    assert_eq!(
        store.save(second.clone(), None),
        Err(HistoryStoreError::Conflict)
    );
    assert_eq!(
        store.save(second.clone(), Some(7)),
        Err(HistoryStoreError::Conflict)
    );
    store.save(second.clone(), Some(1)).unwrap();
    assert_eq!(store.load(&scope).unwrap(), Some(second));

    store.delete_scope(&scope).unwrap();
    assert_eq!(store.load(&scope).unwrap(), None);
    assert!(values.keys(HISTORY_BUCKET).unwrap().is_empty());

    // A value that is no journal is an error, not an empty history.
    values
        .set(HISTORY_BUCKET, scope.as_str(), b"{]".to_vec())
        .unwrap();
    assert!(matches!(
        store.load(&scope),
        Err(HistoryStoreError::Backend(_))
    ));
}

#[test]
fn persistent_history_registers_the_store_over_the_app_s_key_value_store() {
    let services = RayXAppBuilder::new()
        .add_persistent_history()
        .build()
        .unwrap();
    let values = services.required_service::<dyn KeyValueStore>().unwrap();
    let store = services.required_service::<dyn HistoryStore>().unwrap();
    let scope = HistoryScopeId::new("persistence:registered").unwrap();
    store
        .save(HistoryJournalV2::new(1, scope.clone(), Vec::new()), None)
        .unwrap();
    assert!(
        values
            .get(HISTORY_BUCKET, scope.as_str())
            .unwrap()
            .is_some()
    );
}
