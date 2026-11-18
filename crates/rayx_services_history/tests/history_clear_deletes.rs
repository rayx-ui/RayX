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

mod deleting_while_others_write {
    //! A clear's delete meets what happened since the clear decided on it: another history's
    //! save (its entries are kept) and a further edit and clear of this history (nothing is
    //! saved for it).

    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use futures::future::{BoxFuture, LocalBoxFuture};
    use rayx_app::di::{
        AsyncInitializable, StartupInitializable, StartupInitializationContext,
        StartupInitializationResult, StartupServiceType,
    };
    use rayx_app::services::history::{HistoryScopeId, HistoryService};
    use rayx_app::services::operations::{OperationOrigin, OperationService, OperationTargetId};
    use rayx_services_history::{
        HistoryJournalV2, HistoryStore, HistoryStoreError, InMemoryHistoryStore,
    };

    use super::{Set, Target, now};

    const WAIT: Duration = Duration::from_secs(5);

    type Hook = Box<dyn FnOnce() + Send>;

    /// Journals shared by the histories over it, its next delete running `before_delete` first.
    struct GatedStore {
        inner: Arc<InMemoryHistoryStore>,
        before_delete: Mutex<Option<Hook>>,
    }

    impl GatedStore {
        fn over(inner: &Arc<InMemoryHistoryStore>) -> Arc<Self> {
            Arc::new(Self {
                inner: Arc::clone(inner),
                before_delete: Mutex::new(None),
            })
        }

        fn before_next_delete(&self, hook: impl FnOnce() + Send + 'static) {
            *self.before_delete.lock().unwrap() = Some(Box::new(hook));
        }
    }

    impl AsyncInitializable for GatedStore {
        fn is_initialized(&self) -> bool {
            self.inner.is_initialized()
        }
        fn initialize(
            &self,
            context: StartupInitializationContext,
        ) -> LocalBoxFuture<'static, StartupInitializationResult> {
            self.inner.initialize(context)
        }
    }

    impl StartupInitializable for GatedStore {
        fn startup_service_type(&self) -> Option<StartupServiceType> {
            Some(StartupServiceType::of::<dyn HistoryStore>())
        }
    }

    impl HistoryStore for GatedStore {
        fn load<'a>(
            &'a self,
            scope: &'a HistoryScopeId,
        ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
            self.inner.load(scope)
        }
        fn save(
            &self,
            journal: HistoryJournalV2,
            expected_revision: Option<u64>,
        ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
            self.inner.save(journal, expected_revision)
        }
        fn delete_scope<'a>(
            &'a self,
            scope: &'a HistoryScopeId,
            expected_revision: u64,
        ) -> BoxFuture<'a, Result<bool, HistoryStoreError>> {
            let hook = self.before_delete.lock().unwrap().take();
            if let Some(hook) = hook {
                hook();
            }
            self.inner.delete_scope(scope, expected_revision)
        }
    }

    /// A history over `store` with a target at 0 bound to the scope.
    #[derive(Clone)]
    struct Editor {
        operations: Arc<dyn OperationService>,
        history: Arc<dyn HistoryService>,
        target_id: OperationTargetId,
        scope: HistoryScopeId,
    }

    impl Editor {
        fn over(store: Arc<GatedStore>) -> Self {
            let operations: Arc<dyn OperationService> = rayx_app::testing::FakeOperations::new();
            let history =
                rayx_services_history::testing::history_with_store(Arc::clone(&operations), store);
            let target_id = OperationTargetId::new("persistence:target").unwrap();
            let scope = HistoryScopeId::new("persistence:scope").unwrap();
            now(operations.register_handler(target_id.clone(), Arc::new(Target(Mutex::new(0)))))
                .unwrap();
            now(history.bind_target_scope(target_id.clone(), scope.clone())).unwrap();
            Self {
                operations,
                history,
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
    }

    /// How many entries the scope's journal in `store` holds, if one is saved.
    fn saved_entries(store: &InMemoryHistoryStore, scope: &HistoryScopeId) -> Option<usize> {
        now(store.load(scope))
            .unwrap()
            .map(|journal| journal.entries.len())
    }

    #[test]
    fn a_save_landing_before_a_clear_s_delete_keeps_its_entries() {
        let journals = Arc::new(InMemoryHistoryStore::default());
        let first_store = GatedStore::over(&journals);
        let first = Editor::over(Arc::clone(&first_store));
        let second = Editor::over(GatedStore::over(&journals));
        first.set(1);
        assert_eq!(saved_entries(&journals, &first.scope), Some(1));

        // The second history saves its entry after the first's clear decided to delete.
        let saving = second.clone();
        first_store.before_next_delete(move || saving.set(10));
        now(first.history.clear_scope(first.scope.clone())).unwrap();
        assert_eq!(
            saved_entries(&journals, &first.scope),
            Some(1),
            "the second history's entry is kept"
        );

        second.set(11);
        assert_eq!(saved_entries(&journals, &second.scope), Some(2));
        let restarted = Editor::over(GatedStore::over(&journals));
        now(restarted.history.undo(restarted.scope.clone())).unwrap();
        assert_eq!(
            restarted.history.stack_depths(&restarted.scope),
            (1, 1),
            "both of the second history's entries were loaded"
        );
    }

    #[test]
    fn a_clear_made_while_a_delete_runs_saves_no_empty_journal() {
        let journals = Arc::new(InMemoryHistoryStore::default());
        let store = GatedStore::over(&journals);
        let editor = Editor::over(Arc::clone(&store));
        editor.set(1);
        assert_eq!(saved_entries(&journals, &editor.scope), Some(1));

        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel::<()>();
        store.before_next_delete(move || {
            entered_tx.send(()).unwrap();
            resume_rx.recv_timeout(WAIT).unwrap();
        });
        let clearing = editor.clone();
        let first_clear = std::thread::spawn(move || {
            futures::executor::block_on(clearing.history.clear_scope(clearing.scope.clone()))
        });
        entered_rx.recv_timeout(WAIT).unwrap();

        // While the delete runs: an edit, then a second clear (waiting for its save).
        editor.set(2);
        assert_eq!(editor.history.stack_depths(&editor.scope), (1, 0));
        let clearing = editor.clone();
        let second_clear = std::thread::spawn(move || {
            futures::executor::block_on(clearing.history.clear_scope(clearing.scope.clone()))
        });
        let deadline = Instant::now() + WAIT;
        while editor.history.stack_depths(&editor.scope) != (0, 0) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(editor.history.stack_depths(&editor.scope), (0, 0));
        resume_tx.send(()).unwrap();

        first_clear.join().unwrap().unwrap();
        second_clear.join().unwrap().unwrap();
        assert_eq!(
            saved_entries(&journals, &editor.scope),
            None,
            "the cleared scope stays deleted"
        );
    }
}
