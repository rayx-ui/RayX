//! History kept in the app's key-value store (`add_persistent_history`): a second app over the
//! same store undoes what the first one did, and the store's journals are revision-checked. It
//! requires a key-value store and registers none: without one the build names how to add one,
//! and over the app's own (no startup service) it starts, after it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::executor::block_on;
use rayx_app::AppBuilder as _;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::history::{HistoryScopeId, HistoryService};
use rayx_app::services::operations::{
    DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
    OperationTargetHandler, OperationTargetId,
};
use rayx_app::services::storage::KeyValueStore;
use rayx_app::testing::{FakeKeyValueStore, FakeOperations};
use rayx_app::{RayXAppBuilder, ServiceProvider};
use rayx_services_history::{
    HISTORY_BUCKET, HistoryAppBuilderExt, HistoryJournalV2, HistoryStore, HistoryStoreError,
    KeyValueHistoryStore,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.persistence.set";
    const VERSION: u32 = 1;
}

/// What `call` answered: the services here answer at once.
fn now<F: std::future::Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

/// `builder` with `store` as its key-value store and an operation service, which persistent
/// history requires.
fn with_store(mut builder: RayXAppBuilder, store: Arc<FakeKeyValueStore>) -> RayXAppBuilder {
    let services = builder.services_mut();
    services.add_singleton_as::<dyn KeyValueStore>(store as Arc<dyn KeyValueStore>);
    services.add_singleton_as::<dyn OperationService>(
        FakeOperations::new() as Arc<dyn OperationService>
    );
    builder
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
    operations: Arc<dyn OperationService>,
    history: Arc<dyn HistoryService>,
    target: Arc<Target>,
    target_id: OperationTargetId,
    scope: HistoryScopeId,
}

impl App {
    fn start(store: &Arc<FakeKeyValueStore>, value: u64) -> Self {
        let services = with_store(RayXAppBuilder::new(), Arc::clone(store))
            .add_persistent_history()
            .build()
            .unwrap();
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .unwrap();
        let operations = services.required_service::<dyn OperationService>().unwrap();
        let history = services.required_service::<dyn HistoryService>().unwrap();
        let target = Arc::new(Target(Mutex::new(value)));
        let target_id = OperationTargetId::new("persistence:target").unwrap();
        let scope = HistoryScopeId::new("persistence:scope").unwrap();
        now(operations.register_handler(target_id.clone(), target.clone())).unwrap();
        now(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
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
        now(self
            .operations
            .submit(&self.target_id, Set(value), OperationOrigin::LocalUser))
        .unwrap();
    }

    fn value(&self) -> u64 {
        *self.target.0.lock().unwrap()
    }
}

#[test]
fn a_second_app_over_the_same_store_undoes_the_first_app_s_edits() {
    let store = FakeKeyValueStore::new();
    let first = App::start(&store, 0);
    first.set(1);
    first.set(2);
    assert_eq!(first.history.stack_depths(&first.scope), (2, 0));
    assert!(
        block_on(store.get(HISTORY_BUCKET.into(), first.scope.as_str().into()))
            .unwrap()
            .is_some(),
        "the journal is in the app's store"
    );
    drop(first);

    // The app runs again (its document restored at 2): the history came back with the store.
    let second = App::start(&store, 2);
    assert_eq!(second.history.stack_depths(&second.scope), (2, 0));
    assert!(now(second.history.undo(second.scope.clone())).unwrap());
    assert_eq!(second.value(), 1);
    assert!(now(second.history.undo(second.scope.clone())).unwrap());
    assert_eq!(second.value(), 0);
    assert!(!now(second.history.undo(second.scope.clone())).unwrap());
}

#[test]
fn the_store_checks_revisions_and_deletes_scopes() {
    let values: Arc<dyn KeyValueStore> = FakeKeyValueStore::new();
    let store = KeyValueHistoryStore::new(values.clone());
    let scope = HistoryScopeId::new("persistence:direct").unwrap();
    assert_eq!(now(store.load(&scope)).unwrap(), None);

    let first = HistoryJournalV2::new(1, scope.clone(), Vec::new());
    now(store.save(first.clone(), None)).unwrap();
    assert_eq!(now(store.load(&scope)).unwrap(), Some(first));
    // A save that expects another revision than the store holds is refused.
    let second = HistoryJournalV2::new(2, scope.clone(), Vec::new());
    assert_eq!(
        now(store.save(second.clone(), None)),
        Err(HistoryStoreError::Conflict)
    );
    assert_eq!(
        now(store.save(second.clone(), Some(7))),
        Err(HistoryStoreError::Conflict)
    );
    now(store.save(second.clone(), Some(1))).unwrap();
    assert_eq!(now(store.load(&scope)).unwrap(), Some(second));

    now(store.delete_scope(&scope)).unwrap();
    assert_eq!(now(store.load(&scope)).unwrap(), None);
    assert!(
        block_on(values.keys(HISTORY_BUCKET.into()))
            .unwrap()
            .is_empty()
    );

    // A value that is no journal is an error, not an empty history.
    block_on(values.set(HISTORY_BUCKET.into(), scope.as_str().into(), b"{]".to_vec())).unwrap();
    assert!(matches!(
        now(store.load(&scope)),
        Err(HistoryStoreError::Backend(_))
    ));
}

#[test]
fn persistent_history_registers_the_store_over_the_app_s_key_value_store() {
    let services = with_store(RayXAppBuilder::new(), FakeKeyValueStore::new())
        .add_persistent_history()
        .build()
        .unwrap();
    let values = services.required_service::<dyn KeyValueStore>().unwrap();
    let store = services.required_service::<dyn HistoryStore>().unwrap();
    let scope = HistoryScopeId::new("persistence:registered").unwrap();
    now(store.save(HistoryJournalV2::new(1, scope.clone(), Vec::new()), None)).unwrap();
    assert!(
        block_on(values.get(HISTORY_BUCKET.into(), scope.as_str().into()))
            .unwrap()
            .is_some()
    );
}

#[test]
fn without_a_store_the_build_names_what_to_add() {
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn OperationService>(
            FakeOperations::new() as Arc<dyn OperationService>
        );
    let Err(error) = builder.add_persistent_history().build() else {
        panic!("the build fails");
    };
    let error = error.to_string();
    assert!(error.contains("HistoryStore"), "{error}");
    assert!(error.contains("KeyValueStore"), "{error}");
    assert!(error.contains("add_storage()"), "{error}");
    assert!(error.contains("rayx_services_storage"), "{error}");
}

#[test]
fn persistent_history_registers_no_store() {
    let mut builder = RayXAppBuilder::new().add_persistent_history();
    assert!(
        !builder
            .services_mut()
            .contains_service::<dyn KeyValueStore>()
    );
}

#[test]
fn over_the_app_s_own_store_it_starts() {
    let services = with_store(RayXAppBuilder::new(), FakeKeyValueStore::new())
        .add_persistent_history()
        .build()
        .unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    assert!(services.required_service::<dyn HistoryStore>().is_ok());
    assert!(services.required_service::<dyn HistoryService>().is_ok());
}
