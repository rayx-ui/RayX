//! Clearing a scope forgets it in the history's store (`delete_scope`) instead of saving an empty
//! journal, over the app's key-value store (`add_persistent_history`); a scope that still holds
//! another history's entries is saved with them; a later edit saves the scope afresh.

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
use rayx_services_history::{HISTORY_BUCKET, HistoryAppBuilderExt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
struct Set(u64);

impl DomainOperation for Set {
    const KIND: &'static str = "history.clear.set";
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

/// The scope's journal in `store`'s history bucket, if one is saved.
fn saved(store: &FakeKeyValueStore, scope: &HistoryScopeId) -> Option<serde_json::Value> {
    block_on(store.get(HISTORY_BUCKET.into(), scope.as_str().into()))
        .unwrap()
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn clearing_a_scope_deletes_its_journal() {
    let store = FakeKeyValueStore::new();
    let app = App::start(&store, 0);
    app.set(1);
    app.set(2);
    assert_eq!(
        saved(&store, &app.scope).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    now(app.history.clear_scope(app.scope.clone())).unwrap();
    assert_eq!(saved(&store, &app.scope), None, "the scope is forgotten");
    assert!(
        block_on(store.keys(HISTORY_BUCKET.into()))
            .unwrap()
            .is_empty()
    );

    // An edit after it saves the scope afresh.
    app.set(3);
    assert_eq!(
        saved(&store, &app.scope).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(now(app.history.undo(app.scope.clone())).unwrap());
    assert_eq!(app.value(), 2);
}

#[test]
fn clearing_keeps_another_history_s_entries() {
    let store = FakeKeyValueStore::new();
    let first = App::start(&store, 0);
    let second = App::start(&store, 0);
    first.set(1);
    second.set(2);
    assert_eq!(
        saved(&store, &first.scope).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    now(first.history.clear_scope(first.scope.clone())).unwrap();
    let kept = saved(&store, &first.scope).expect("the second history's entry is kept");
    assert_eq!(kept["entries"].as_array().unwrap().len(), 1);

    now(second.history.clear_scope(second.scope.clone())).unwrap();
    assert_eq!(
        saved(&store, &second.scope),
        None,
        "nothing is left: forgotten"
    );
}
