//! The history service: undo and redo per scope, journals, persistence and scope lifecycle.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod service_contracts {
    use std::sync::{Arc, Mutex};

    use rayx_runtime::services::{
        history::{HistoryScopeId, HistoryService},
        operations::{
            DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
            OperationTargetHandler, OperationTargetId,
        },
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct SetValue {
        value: i64,
    }

    impl DomainOperation for SetValue {
        const KIND: &'static str = "history.set-value";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct ValueTarget(Mutex<i64>);

    struct NoOpTarget;

    impl OperationTargetHandler<SetValue> for ValueTarget {
        fn apply_atomically(
            &self,
            operation: &SetValue,
            _origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<SetValue>, String> {
            let mut value = self.0.lock().unwrap();
            let before = *value;
            *value = operation.value;
            Ok(OperationApplyResult::applied(
                SetValue { value: before },
                ["history.value"],
                before as u64,
                operation.value as u64,
            ))
        }
    }

    impl OperationTargetHandler<SetValue> for NoOpTarget {
        fn apply_atomically(
            &self,
            _operation: &SetValue,
            _origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<SetValue>, String> {
            Ok(OperationApplyResult::no_op(0))
        }
    }

    #[test]
    fn local_receipts_record_in_an_explicit_scope_and_undo_redo_resubmit_operations() {
        let operations = OperationService::new();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("history:sample").unwrap();
        operations
            .register_target(target_id.clone(), target.clone())
            .unwrap();
        let history = HistoryService::new(operations.clone()).unwrap();
        let no_op_target_id = OperationTargetId::new("history:no-op").unwrap();
        operations
            .register_target(no_op_target_id.clone(), Arc::new(NoOpTarget))
            .unwrap();
        let scope = HistoryScopeId::new("property-grid:sample").unwrap();
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap();

        operations
            .submit(
                &target_id,
                SetValue { value: 5 },
                OperationOrigin::LocalUser,
            )
            .unwrap();
        operations
            .submit(
                &no_op_target_id,
                SetValue { value: 10 },
                OperationOrigin::LocalUser,
            )
            .unwrap();
        assert!(history.can_undo(&scope).unwrap());
        assert!(history.can_undo(&scope).unwrap());
        #[cfg(feature = "diagnostics")]
        assert_eq!(history.diagnostics(&scope).stack_revision, 1);

        assert!(history.undo(&scope).unwrap());
        assert_eq!(*target.0.lock().unwrap(), 0);
        assert!(history.can_redo(&scope).unwrap());
        assert!(!history.can_undo(&scope).unwrap());
        assert!(history.can_redo(&scope).unwrap());
        #[cfg(feature = "diagnostics")]
        assert_eq!(history.diagnostics(&scope).stack_revision, 2);

        assert!(history.redo(&scope).unwrap());
        assert_eq!(*target.0.lock().unwrap(), 5);
        assert!(history.can_undo(&scope).unwrap());
        assert!(!history.can_redo(&scope).unwrap());
        #[cfg(feature = "diagnostics")]
        assert_eq!(history.diagnostics(&scope).stack_revision, 3);
    }

    #[test]
    fn remote_and_history_receipts_do_not_reenter_local_history() {
        let operations = OperationService::new();
        let target_id = OperationTargetId::new("history:remote").unwrap();
        operations
            .register_target(target_id.clone(), Arc::new(ValueTarget::default()))
            .unwrap();
        let history = HistoryService::new(operations.clone()).unwrap();

        operations
            .submit(
                &target_id,
                SetValue { value: 5 },
                OperationOrigin::Remote("peer-a".to_string()),
            )
            .unwrap();
        operations
            .submit(
                &target_id,
                SetValue { value: 7 },
                OperationOrigin::HistoryRedo,
            )
            .unwrap();
        operations
            .submit(
                &target_id,
                SetValue { value: 9 },
                OperationOrigin::LocalUser,
            )
            .unwrap();

        let scope = HistoryScopeId::new(target_id.as_str()).unwrap();
        assert!(history.can_undo(&scope).unwrap());
    }

    #[test]
    fn transaction_receipts_form_one_entry_and_released_targets_keep_that_entry_available() {
        let operations = OperationService::new();
        let target_id = OperationTargetId::new("history:transaction").unwrap();
        operations
            .register_target(target_id.clone(), Arc::new(ValueTarget::default()))
            .unwrap();
        let history = HistoryService::new(operations.clone()).unwrap();
        let scope = HistoryScopeId::new("inspector:transaction").unwrap();
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap();

        let transaction = operations
            .begin_transaction(rayx_runtime::services::operations::OperationTransactionMode::New)
            .unwrap();
        operations
            .submit(
                &target_id,
                SetValue { value: 2 },
                OperationOrigin::LocalUser,
            )
            .unwrap();
        operations
            .submit(
                &target_id,
                SetValue { value: 3 },
                OperationOrigin::LocalUser,
            )
            .unwrap();
        transaction.commit().unwrap();

        assert!(history.can_undo(&scope).unwrap());
        operations.release_target(&target_id).unwrap();
        let error = history.undo(&scope).unwrap_err();
        assert!(
            matches!(
                error,
                rayx_runtime::services::history::HistoryError::Operation(
                    rayx_runtime::services::operations::OperationError::TargetReleased(_)
                )
            ),
            "history release undo error: {error:?}"
        );
        assert!(history.can_undo(&scope).unwrap());
        assert!(!history.can_redo(&scope).unwrap());
        #[cfg(feature = "diagnostics")]
        assert_eq!(history.diagnostics(&scope).unavailable_entry_count, 1);
    }
}

mod scope_lifecycle {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};

    use futures::future::LocalBoxFuture;
    use rayx_core::di::{
        AsyncInitializable, StartupInitializable, StartupInitializationContext,
        StartupInitializationResult, StartupServiceType,
    };
    use rayx_runtime::services::{
        history::{
            HistoryError, HistoryEvent, HistoryJournalV2, HistoryScopeId, HistoryService,
            HistoryStore, HistoryStoreError, InMemoryHistoryStore,
        },
        operations::{
            DomainOperation, MAX_OPERATION_PAYLOAD_BYTES, OperationApplyResult, OperationError,
            OperationOrigin, OperationRecord, OperationService, OperationTargetHandler,
            OperationTargetId, OperationTransactionMode,
        },
    };
    use serde::{Deserialize, Serialize};

    const WAIT: Duration = Duration::from_secs(5);

    #[derive(Clone, Deserialize, Serialize)]
    struct Set(u64);

    impl DomainOperation for Set {
        const KIND: &'static str = "history.lifecycle.set";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct Target {
        value: Mutex<u64>,
        fail: AtomicBool,
        on_replay: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    }

    impl OperationTargetHandler<Set> for Target {
        fn apply_atomically(
            &self,
            operation: &Set,
            origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<Set>, String> {
            if matches!(
                origin,
                OperationOrigin::HistoryUndo | OperationOrigin::HistoryRedo
            ) && let Some(callback) = self
                .on_replay
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .as_ref()
            {
                callback();
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err("injected replay failure".into());
            }
            let mut value = self
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            let before = *value;
            *value = operation.0;
            Ok(OperationApplyResult::applied(
                Set(before),
                ["lifecycle.value"],
                before,
                operation.0,
            ))
        }
    }

    struct Fixture {
        operations: OperationService,
        history: HistoryService,
        target: Arc<Target>,
        target_id: OperationTargetId,
        scope: HistoryScopeId,
    }

    impl Fixture {
        fn new(store: Option<Arc<dyn HistoryStore>>) -> Self {
            let operations = OperationService::new();
            let target = Arc::new(Target::default());
            let target_id = OperationTargetId::new("lifecycle:target")
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            let scope = HistoryScopeId::new("lifecycle:scope")
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            operations
                .register_target(target_id.clone(), target.clone())
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            let history = match store {
                Some(store) => HistoryService::with_store(operations.clone(), store)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
                None => HistoryService::new(operations.clone())
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            };
            history
                .bind_target_scope(target_id.clone(), scope.clone())
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            Self {
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
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        }

        fn depths(&self) -> (usize, usize) {
            self.history
                .stack_depths(&self.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        }

        fn value(&self) -> u64 {
            *self
                .target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        }
    }

    #[test]
    fn limits_zero_one_and_256_bound_257_commits_without_changing_replay_order() {
        let f = Fixture::new(None);
        f.history
            .set_scope_entry_limit(&f.scope, 1)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.set(1);
        assert!(matches!(
            f.history.set_scope_entry_limit(&f.scope, 0),
            Err(HistoryError::InvalidEntryLimit)
        ));
        assert_eq!(f.depths(), (1, 0));
        assert_eq!(f.value(), 1);
        f.set(2);
        assert_eq!(f.depths(), (1, 0));
        assert!(
            f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(f.value(), 1);
        assert!(
            !f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        f.history
            .clear_scope(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.history
            .set_scope_entry_limit(&f.scope, 256)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        for value in 2..=258 {
            f.set(value);
        }
        assert_eq!(f.depths(), (256, 0));
        for _ in 0..256 {
            assert!(
                f.history
                    .undo(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            );
        }
        assert_eq!(f.value(), 2);
        assert_eq!(f.depths(), (0, 256));
        assert!(
            !f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        for _ in 0..256 {
            assert!(
                f.history
                    .redo(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            );
        }
        assert_eq!(f.value(), 258);
        assert_eq!(f.depths(), (256, 0));
    }

    #[test]
    fn shrink_evicts_old_undo_then_distant_redo_and_preserves_next_replay() {
        let f = Fixture::new(None);
        for value in 1..=5 {
            f.set(value);
        }
        for _ in 0..3 {
            f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        }
        assert_eq!(f.depths(), (2, 3));
        f.history
            .set_scope_entry_limit(&f.scope, 2)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.depths(), (0, 2));
        assert_eq!(f.value(), 2);
        assert!(
            f.history
                .redo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(f.value(), 3);
        assert!(
            f.history
                .redo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(f.value(), 4);
        assert!(
            !f.history
                .redo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
    }

    #[test]
    fn grouped_transactions_are_evicted_whole_and_other_scopes_are_unchanged() {
        let f = Fixture::new(None);
        let other = HistoryScopeId::new("lifecycle:other")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let other_id = OperationTargetId::new("lifecycle:other-target")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let other_target = Arc::new(Target::default());
        f.operations
            .register_target(other_id.clone(), other_target.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.history
            .bind_target_scope(other_id.clone(), other.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.history
            .set_scope_entry_limit(&f.scope, 1)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let transaction = f
            .operations
            .begin_transaction(OperationTransactionMode::New)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.set(1);
        f.set(2);
        transaction
            .commit()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.depths(), (1, 0));
        f.operations
            .submit(&other_id, Set(9), OperationOrigin::LocalUser)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.set(3);
        f.history
            .undo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.value(), 2);
        assert!(
            !f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        f.history
            .clear_scope(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(
            f.history
                .stack_depths(&other)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            (1, 0)
        );
        assert_eq!(
            *other_target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            9
        );
        f.history
            .undo(&other)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(
            *other_target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            0
        );
    }

    #[test]
    fn clear_and_limit_events_allow_reentrant_reads_and_preserve_binding_and_limit() {
        let f = Fixture::new(None);
        f.set(1);
        f.set(2);
        f.history
            .undo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let (events_tx, events_rx) = mpsc::channel();
        let history = f.history.clone();
        let scope = f.scope.clone();
        let _subscription = f
            .history
            .subscribe(move |event| {
                events_tx
                    .send((
                        event.clone(),
                        history
                            .stack_depths(&scope)
                            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
                    ))
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            })
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let history = f.history.clone();
        let scope = f.scope.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            history
                .set_scope_entry_limit(&scope, 1)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            history
                .clear_scope(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            done_tx
                .send(())
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        });
        done_rx.recv_timeout(WAIT).unwrap_or_else(|error| {
            panic!("lifecycle callbacks must not hold the history lock: {error:?}")
        });
        worker
            .join()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let events: Vec<_> = events_rx.try_iter().collect();
        assert!(events.iter().any(
            |(event, _)| matches!(event, HistoryEvent::AvailabilityChanged(scope) if scope == &f.scope)
        ));
        assert!(events.iter().any(
            |(event, depths)| matches!(event, HistoryEvent::ScopeCleared(scope) if scope == &f.scope)
                && *depths == (0, 0)
        ));
        assert_eq!(f.depths(), (0, 0));
        assert!(
            !f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert!(
            !f.history
                .redo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        f.set(3);
        f.set(4);
        assert_eq!(f.depths(), (1, 0));
        f.history
            .undo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.value(), 3);
    }

    #[test]
    fn replay_failures_and_reentrant_lifecycle_attempts_preserve_stacks() {
        let f = Fixture::new(None);
        f.set(1);
        f.target.fail.store(true, Ordering::SeqCst);
        assert!(f.history.undo(&f.scope).is_err());
        assert_eq!(f.depths(), (1, 0));
        assert_eq!(f.value(), 1);
        f.target.fail.store(false, Ordering::SeqCst);
        let history = f.history.clone();
        let scope = f.scope.clone();
        *f.target
            .on_replay
            .lock()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")) =
            Some(Box::new(move || {
                assert!(matches!(
                    history.clear_scope(&scope),
                    Err(HistoryError::ScopeBusy)
                ));
                assert!(matches!(
                    history.set_scope_entry_limit(&scope, 1),
                    Err(HistoryError::ScopeBusy)
                ));
            }));
        assert!(
            f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(f.depths(), (0, 1));
        assert!(
            f.history
                .redo(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(f.depths(), (1, 0));
        *f.target
            .on_replay
            .lock()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")) = None;
        let transaction = f
            .operations
            .begin_transaction(OperationTransactionMode::New)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(matches!(
            f.history.clear_scope(&f.scope),
            Err(HistoryError::ScopeBusy)
        ));
        assert!(matches!(
            f.history.set_scope_entry_limit(&f.scope, 1),
            Err(HistoryError::ScopeBusy)
        ));
        transaction
            .commit()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.depths(), (1, 0));
    }

    #[test]
    fn grouped_record_notification_cannot_clear_a_still_pending_group() {
        let f = Fixture::new(None);
        let history = f.history.clone();
        let scope = f.scope.clone();
        let (tx, rx) = mpsc::channel();
        let _subscription = f
            .history
            .subscribe(move |event| {
                if matches!(event, HistoryEvent::Recorded(_)) {
                    tx.send(history.clear_scope(&scope))
                        .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
                }
            })
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let transaction = f
            .operations
            .begin_transaction(OperationTransactionMode::New)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.set(1);
        f.set(2);
        transaction
            .commit()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(matches!(
            rx.recv_timeout(WAIT)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            Err(HistoryError::ScopeBusy)
        ));
        assert_eq!(f.depths(), (1, 0));
        f.history
            .undo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.value(), 0);
    }

    #[derive(Default)]
    struct ControlledStore {
        inner: InMemoryHistoryStore,
        fail: AtomicBool,
        blocked: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    }

    impl AsyncInitializable for ControlledStore {
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

    impl StartupInitializable for ControlledStore {
        fn startup_service_type(&self) -> StartupServiceType {
            StartupServiceType::of::<dyn HistoryStore>()
        }
    }

    impl HistoryStore for ControlledStore {
        fn load(
            &self,
            scope: &HistoryScopeId,
        ) -> Result<Option<HistoryJournalV2>, HistoryStoreError> {
            self.inner.load(scope)
        }
        fn save(
            &self,
            journal: HistoryJournalV2,
            expected_revision: Option<u64>,
        ) -> Result<(), HistoryStoreError> {
            let blocked = self
                .blocked
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .take();
            if let Some((entered, resume)) = blocked {
                entered
                    .send(())
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
                resume
                    .recv_timeout(WAIT)
                    .map_err(|_| HistoryStoreError::Backend("save release timed out".into()))?;
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err(HistoryStoreError::Backend("injected save failure".into()));
            }
            self.inner.save(journal, expected_revision)
        }
        fn delete_scope(&self, scope: &HistoryScopeId) -> Result<(), HistoryStoreError> {
            self.inner.delete_scope(scope)
        }
    }

    #[test]
    fn clear_reports_store_failure_without_resurrecting_local_history_and_retry_persists() {
        let store = Arc::new(ControlledStore::default());
        let f = Fixture::new(Some(store.clone()));
        f.set(1);
        store.fail.store(true, Ordering::SeqCst);
        assert!(matches!(
            f.history.clear_scope(&f.scope),
            Err(HistoryError::Store(_))
        ));
        assert_eq!(f.depths(), (0, 0));
        assert_eq!(f.value(), 1);
        store.fail.store(false, Ordering::SeqCst);
        f.history
            .clear_scope(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(
            store
                .load(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .unwrap_or_else(|| panic!("expected persisted history journal"))
                .entries
                .is_empty()
        );
        let restarted = Fixture::new(Some(store));
        assert_eq!(restarted.depths(), (0, 0));
    }

    #[test]
    fn a_pending_old_save_cannot_overwrite_a_later_clear() {
        let store = Arc::new(ControlledStore::default());
        let f = Fixture::new(Some(store.clone()));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        *store
            .blocked
            .lock()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")) =
            Some((entered_tx, resume_rx));
        let operations = f.operations.clone();
        let target_id = f.target_id.clone();
        let writer = std::thread::spawn(move || {
            operations.submit(&target_id, Set(1), OperationOrigin::LocalUser)
        });
        entered_rx
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let history = f.history.clone();
        let scope = f.scope.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let clearer = std::thread::spawn(move || {
            done_tx
                .send(history.clear_scope(&scope))
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        });
        let deadline = Instant::now() + WAIT;
        while f.depths() != (0, 0) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let cleared_before_save = f.depths() == (0, 0);
        resume_tx
            .send(())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(
            cleared_before_save,
            "clear must update local stacks before waiting for persistence"
        );
        writer
            .join()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        done_rx
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        clearer
            .join()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let journal = store
            .load(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            .unwrap_or_else(|| panic!("expected persisted history journal"));
        assert!(journal.entries.is_empty());
        let restarted = Fixture::new(Some(store));
        assert_eq!(restarted.depths(), (0, 0));
    }

    #[test]
    fn a_replay_on_another_thread_keeps_scope_lifecycle_busy_until_completion() {
        let f = Fixture::new(None);
        f.set(1);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let resume_rx = Mutex::new(resume_rx);
        *f.target
            .on_replay
            .lock()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")) =
            Some(Box::new(move || {
                entered_tx
                    .send(())
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
                resume_rx
                    .lock()
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                    .recv_timeout(WAIT)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            }));
        let history = f.history.clone();
        let scope = f.scope.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let replay = std::thread::spawn(move || {
            done_tx
                .send(history.undo(&scope))
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        });
        entered_rx
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let clear = f.history.clear_scope(&f.scope);
        let limit = f.history.set_scope_entry_limit(&f.scope, 1);
        resume_tx
            .send(())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(matches!(clear, Err(HistoryError::ScopeBusy)));
        assert!(matches!(limit, Err(HistoryError::ScopeBusy)));
        assert!(
            done_rx
                .recv_timeout(WAIT)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        replay
            .join()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        *f.target
            .on_replay
            .lock()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")) = None;
        assert_eq!(f.depths(), (0, 1));
        assert_eq!(f.value(), 0);
    }

    #[test]
    fn clear_discards_a_transaction_started_on_another_thread_before_its_boundary() {
        let f = Fixture::new(None);
        let operations = f.operations.clone();
        let target_id = f.target_id.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (commit_tx, commit_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            let transaction = operations
                .begin_transaction(OperationTransactionMode::New)
                .unwrap_or_else(|error| panic!("begin transaction failed: {error:?}"));
            for value in [1, 2] {
                operations
                    .submit(&target_id, Set(value), OperationOrigin::LocalUser)
                    .unwrap_or_else(|error| panic!("transaction edit failed: {error:?}"));
            }
            ready_tx
                .send(())
                .unwrap_or_else(|error| panic!("signal failed: {error:?}"));
            commit_rx
                .recv_timeout(WAIT)
                .unwrap_or_else(|error| panic!("commit release timed out: {error:?}"));
            done_tx
                .send(transaction.commit())
                .unwrap_or_else(|error| panic!("completion signal failed: {error:?}"));
        });
        ready_rx
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("transaction never became ready: {error:?}"));
        f.history
            .clear_scope(&f.scope)
            .unwrap_or_else(|error| panic!("clear failed: {error:?}"));
        commit_tx
            .send(())
            .unwrap_or_else(|error| panic!("commit signal failed: {error:?}"));
        done_rx
            .recv_timeout(WAIT)
            .unwrap_or_else(|error| panic!("commit timed out: {error:?}"))
            .unwrap_or_else(|error| panic!("commit failed: {error:?}"));
        writer
            .join()
            .unwrap_or_else(|error| panic!("writer failed: {error:?}"));
        assert_eq!(f.depths(), (0, 0));
        assert_eq!(f.value(), 2);
        f.set(3);
        assert_eq!(f.depths(), (1, 0));
        assert!(
            f.history
                .undo(&f.scope)
                .unwrap_or_else(|error| panic!("undo failed: {error:?}"))
        );
        assert_eq!(f.value(), 2);
    }

    fn retained_set_values(records: &[OperationRecord]) -> Vec<u64> {
        records
            .iter()
            .map(|record| {
                record
                    .decode::<Set>()
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                    .0
            })
            .collect()
    }

    #[test]
    fn scope_retained_records_snapshot_preserves_order_and_tracks_replay_eviction_clear_and_branch()
    {
        let f = Fixture::new(None);
        let empty = HistoryScopeId::new("lifecycle:empty")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(
            f.history
                .retained_records(&empty)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .is_empty()
        );
        f.set(1);
        let transaction = f
            .operations
            .begin_transaction(OperationTransactionMode::New)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.set(2);
        f.set(3);
        transaction
            .commit()
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        f.set(4);
        let snapshot = f
            .history
            .retained_records(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(retained_set_values(&snapshot), [3, 4, 1, 2, 2, 3, 0, 1]);
        assert!(
            snapshot
                .iter()
                .all(|record| record.target_id == f.target_id)
        );
        assert_eq!(f.depths(), (3, 0));
        let events = Arc::new(AtomicUsize::new(0));
        let recorded_events = events.clone();
        let history = f.history.clone();
        let scope = f.scope.clone();
        let _subscription = f
            .history
            .subscribe(move |_| {
                history.retained_records(&scope).unwrap_or_else(|error| {
                    panic!("retained records must be readable from callbacks: {error:?}")
                });
                recorded_events.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        #[cfg(feature = "diagnostics")]
        let revision = f.history.diagnostics(&f.scope).stack_revision;
        for _ in 0..4 {
            assert_eq!(
                f.history
                    .retained_records(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
                snapshot
            );
        }
        assert_eq!(events.load(Ordering::SeqCst), 0);
        assert_eq!(f.value(), 4);
        assert_eq!(f.depths(), (3, 0));
        #[cfg(feature = "diagnostics")]
        assert_eq!(f.history.diagnostics(&f.scope).stack_revision, revision);
        f.history
            .undo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(
            retained_set_values(
                &f.history
                    .retained_records(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            ),
            [1, 2, 2, 3, 0, 1, 3, 4]
        );
        f.history
            .undo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.value(), 1);
        assert_eq!(f.depths(), (1, 2));
        assert_eq!(
            retained_set_values(
                &f.history
                    .retained_records(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            ),
            [0, 1, 1, 2, 2, 3, 3, 4]
        );
        f.history
            .set_scope_entry_limit(&f.scope, 2)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.depths(), (0, 2));
        assert_eq!(
            retained_set_values(
                &f.history
                    .retained_records(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            ),
            [1, 2, 2, 3, 3, 4]
        );
        f.history
            .redo(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(f.value(), 3);
        assert_eq!(f.depths(), (1, 1));
        f.set(9);
        assert_eq!(f.depths(), (2, 0));
        assert_eq!(
            retained_set_values(
                &f.history
                    .retained_records(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            ),
            [3, 9, 1, 2, 2, 3]
        );
        f.history
            .set_scope_entry_limit(&f.scope, 1)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(
            retained_set_values(
                &f.history
                    .retained_records(&f.scope)
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            ),
            [3, 9]
        );
        f.history
            .clear_scope(&f.scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert!(
            f.history
                .retained_records(&f.scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .is_empty()
        );
        assert_eq!(f.depths(), (0, 0));
        assert_eq!(f.value(), 9);
        assert_eq!(
            retained_set_values(&snapshot),
            [3, 4, 1, 2, 2, 3, 0, 1],
            "an owned snapshot remains valid after live history releases its entries"
        );
        assert!(events.load(Ordering::SeqCst) > 0);
    }

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct LimitedText(String);
    impl DomainOperation for LimitedText {
        const KIND: &'static str = "history.lifecycle.limited-text";
        const VERSION: u32 = 1;
    }

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct UnboundedText(String);
    impl DomainOperation for UnboundedText {
        const KIND: &'static str = "history.lifecycle.unbounded-text";
        const VERSION: u32 = 1;
        const MAX_PAYLOAD_BYTES: Option<usize> = None;
    }

    #[derive(Default)]
    struct TextTarget {
        value: Mutex<String>,
        applies: AtomicUsize,
    }
    impl TextTarget {
        fn replace(&self, value: &str) -> (String, u64) {
            let previous = std::mem::replace(
                &mut *self
                    .value
                    .lock()
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
                value.to_owned(),
            );
            let revision = self.applies.fetch_add(1, Ordering::SeqCst) as u64;
            (previous, revision)
        }
    }
    impl OperationTargetHandler<LimitedText> for TextTarget {
        fn apply_atomically(
            &self,
            operation: &LimitedText,
            _: &OperationOrigin,
        ) -> Result<OperationApplyResult<LimitedText>, String> {
            let (previous, revision) = self.replace(&operation.0);
            Ok(OperationApplyResult::applied(
                LimitedText(previous),
                ["lifecycle.text"],
                revision,
                revision + 1,
            ))
        }
    }
    impl OperationTargetHandler<UnboundedText> for TextTarget {
        fn apply_atomically(
            &self,
            operation: &UnboundedText,
            _: &OperationOrigin,
        ) -> Result<OperationApplyResult<UnboundedText>, String> {
            let (previous, revision) = self.replace(&operation.0);
            Ok(OperationApplyResult::applied(
                UnboundedText(previous),
                ["lifecycle.text"],
                revision,
                revision + 1,
            ))
        }
    }

    #[test]
    fn scope_default_payload_limit_rejects_oversized_submission_before_target_or_history_change() {
        let operations = OperationService::new();
        let history = HistoryService::new(operations.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let target = Arc::new(TextTarget::default());
        let target_id = OperationTargetId::new("lifecycle:limited-text")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let scope = HistoryScopeId::new("lifecycle:limited-text")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        operations
            .register_target::<LimitedText, _>(target_id.clone(), target.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        operations
            .submit(
                &target_id,
                LimitedText("baseline".into()),
                OperationOrigin::LocalUser,
            )
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let before = history
            .retained_records(&scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let operation = LimitedText("x".repeat(MAX_OPERATION_PAYLOAD_BYTES + 1));
        let encoded_size = serde_json::to_vec(&operation)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
            .len();
        assert!(
            matches!(operations.submit(&target_id, operation, OperationOrigin::LocalUser), Err(OperationError::PayloadTooLarge { size, limit }) if size == encoded_size && limit == MAX_OPERATION_PAYLOAD_BYTES)
        );
        assert_eq!(
            target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .as_str(),
            "baseline"
        );
        assert_eq!(target.applies.load(Ordering::SeqCst), 1);
        assert_eq!(
            history
                .stack_depths(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            (1, 0)
        );
        assert_eq!(
            history
                .retained_records(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            before
        );
    }

    #[test]
    fn scope_opted_in_large_forward_and_inverse_replay_with_unchanged_record_wire_shape() {
        let operations = OperationService::new();
        let history = HistoryService::new(operations.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let initial = "a".repeat(MAX_OPERATION_PAYLOAD_BYTES + 17);
        let updated = "b".repeat(MAX_OPERATION_PAYLOAD_BYTES + 31);
        let target = Arc::new(TextTarget {
            value: Mutex::new(initial.clone()),
            applies: AtomicUsize::new(0),
        });
        let target_id = OperationTargetId::new("lifecycle:unbounded-text")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let scope = HistoryScopeId::new("lifecycle:unbounded-text")
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        operations
            .register_target::<UnboundedText, _>(target_id.clone(), target.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        operations
            .submit(
                &target_id,
                UnboundedText(updated.clone()),
                OperationOrigin::LocalUser,
            )
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        let records = history
            .retained_records(&scope)
            .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
        assert_eq!(records.len(), 2);
        for (record, text) in records.iter().zip([&initial, &updated]) {
            assert!(record.payload_len() > MAX_OPERATION_PAYLOAD_BYTES);
            assert_eq!(
                record
                    .decode::<UnboundedText>()
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
                UnboundedText(text.clone())
            );
            let expected_bytes = serde_json::to_vec(&UnboundedText(text.clone()))
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            let wire = serde_json::to_value(record)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            assert_eq!(
                wire,
                serde_json::json!({"target_id": target_id.as_str(), "kind": UnboundedText::KIND, "version": UnboundedText::VERSION, "payload": expected_bytes})
            );
            let restored: OperationRecord = serde_json::from_value(wire)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"));
            assert_eq!(&restored, record);
            assert_eq!(
                restored
                    .decode::<UnboundedText>()
                    .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
                UnboundedText(text.clone())
            );
        }
        assert_eq!(
            target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .as_str(),
            updated
        );
        assert!(
            history
                .undo(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(
            target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .as_str(),
            initial
        );
        assert_eq!(
            history
                .stack_depths(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            (0, 1)
        );
        assert!(
            history
                .redo(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
        );
        assert_eq!(
            target
                .value
                .lock()
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}"))
                .as_str(),
            updated
        );
        assert_eq!(target.applies.load(Ordering::SeqCst), 3);
        assert_eq!(
            history
                .stack_depths(&scope)
                .unwrap_or_else(|error| panic!("test prerequisite failed: {error:?}")),
            (1, 0)
        );
    }
}

mod persistence {
    use rayx_runtime::services::{
        history::{
            HistoryJournalError, HistoryJournalV2, HistoryLimits, HistoryScopeId, HistoryService,
            HistoryStore, InMemoryHistoryStore, enforce,
        },
        operations::{
            DomainOperation, OperationApplyResult, OperationOrigin, OperationRecord,
            OperationService, OperationTargetHandler, OperationTargetId,
        },
    };
    use serde::{Deserialize, Serialize};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Deserialize, Serialize)]
    struct Set {
        value: u8,
    }
    impl DomainOperation for Set {
        const KIND: &'static str = "history.persistence.set";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct ValueTarget(Mutex<u8>);

    impl ValueTarget {
        fn with_value(value: u8) -> Self {
            Self(Mutex::new(value))
        }
    }

    impl OperationTargetHandler<Set> for ValueTarget {
        fn apply_atomically(
            &self,
            operation: &Set,
            _origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<Set>, String> {
            let mut value = self.0.lock().unwrap();
            let before = *value;
            *value = operation.value;
            Ok(OperationApplyResult::applied(
                Set { value: before },
                ["history.persistence.value"],
                u64::from(before),
                u64::from(operation.value),
            ))
        }
    }

    #[test]
    fn journal_round_trip_rejects_corrupt_and_future_versions() {
        let scope = HistoryScopeId::new("history:persistence").unwrap();
        let journal = HistoryJournalV2::new(1, scope, vec![]);
        assert_eq!(
            HistoryJournalV2::decode(&journal.encode().unwrap()).unwrap(),
            journal
        );
        assert_eq!(
            HistoryJournalV2::decode(b"bad").unwrap_err(),
            HistoryJournalError::Corrupt
        );
        assert_eq!(
            HistoryJournalV2::decode(br#"{"version":1,"revision":0,"scope_id":"x","entries":[]}"#)
                .unwrap_err(),
            HistoryJournalError::UnsupportedVersion
        );
    }

    #[test]
    fn store_rejects_stale_revision_and_limits_evict_oldest_entry() {
        let scope = HistoryScopeId::new("history:store").unwrap();
        let store = InMemoryHistoryStore::default();
        let first = HistoryJournalV2::new(1, scope.clone(), vec![]);
        store.save(first, None).unwrap();
        assert!(
            store
                .save(HistoryJournalV2::new(2, scope.clone(), vec![]), None)
                .is_err()
        );
        let target = OperationTargetId::new("history:target").unwrap();
        let record = OperationRecord::from_operation(target.clone(), &Set { value: 1 }).unwrap();
        let entry = rayx_runtime::services::history::HistoryEntry {
            transaction_id: 1,
            scope_id: scope,
            target_id: target,
            label_key: "set".into(),
            forward: vec![record.clone()],
            inverse: vec![record],
            before_revision: 0,
            after_revision: 1,
        };
        let mut entries = vec![entry.clone(), entry];
        assert_eq!(
            enforce(
                &mut entries,
                HistoryLimits {
                    max_entries_per_scope: 1,
                    max_bytes_per_scope: 4096
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn focused_history_restores_from_the_app_store_after_restart() {
        let store = Arc::new(InMemoryHistoryStore::default());
        let scope = HistoryScopeId::new("history:restart").unwrap();
        let target_id = OperationTargetId::new("history:restart-target").unwrap();

        let first_operations = OperationService::new();
        first_operations
            .register_target(target_id.clone(), Arc::new(ValueTarget::default()))
            .unwrap();
        let first_history =
            HistoryService::with_store(first_operations.clone(), store.clone()).unwrap();
        first_history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap();
        first_operations
            .submit(&target_id, Set { value: 7 }, OperationOrigin::LocalUser)
            .unwrap();
        assert!(first_history.can_undo(&scope).unwrap());
        drop(first_history);

        let restarted_operations = OperationService::new();
        let restarted_target = Arc::new(ValueTarget::with_value(7));
        restarted_operations
            .register_target(target_id.clone(), restarted_target.clone())
            .unwrap();
        let restarted_history =
            HistoryService::with_store(restarted_operations, store.clone()).unwrap();
        restarted_history
            .bind_target_scope(target_id, scope.clone())
            .unwrap();
        assert!(restarted_history.can_undo(&scope).unwrap());
        assert!(restarted_history.undo(&scope).unwrap());
        assert_eq!(*restarted_target.0.lock().unwrap(), 0);
    }
}

mod receipt_admission {
    use std::sync::{Arc, Mutex};

    use rayx_runtime::services::{
        history::{HistoryScopeId, HistoryService},
        operations::{
            DomainOperation, OperationApplyResult, OperationApplyStatus, OperationOrigin,
            OperationService, OperationTargetHandler, OperationTargetId, OperationTransactionMode,
        },
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct SetValue(i32);

    impl DomainOperation for SetValue {
        const KIND: &'static str = "history.admission.set";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct ValueTarget(Mutex<i32>);

    impl OperationTargetHandler<SetValue> for ValueTarget {
        fn apply_atomically(
            &self,
            operation: &SetValue,
            _: &OperationOrigin,
        ) -> Result<OperationApplyResult<SetValue>, String> {
            let mut value = self.0.lock().map_err(|_| "poisoned".to_string())?;
            let previous = *value;
            if previous == operation.0 {
                return Ok(OperationApplyResult::no_op(previous as u64));
            }
            *value = operation.0;
            Ok(OperationApplyResult::applied(
                SetValue(previous),
                std::iter::empty::<String>(),
                previous.max(0) as u64,
                operation.0.max(0) as u64,
            ))
        }
    }
    #[test]
    fn only_applied_local_user_receipts_with_inverses_enter_one_transaction_entry() {
        let operations = OperationService::new();
        let target_id = OperationTargetId::new("history:admission").unwrap();
        let target = Arc::new(ValueTarget::default());
        operations
            .register_target(target_id.clone(), target.clone())
            .unwrap();
        let history = HistoryService::new(operations.clone()).unwrap();
        let scope = HistoryScopeId::new("history:admission").unwrap();
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap();

        for origin in [
            OperationOrigin::Remote("peer".into()),
            OperationOrigin::System,
            OperationOrigin::Import,
            OperationOrigin::HistoryUndo,
            OperationOrigin::HistoryRedo,
        ] {
            operations.submit(&target_id, SetValue(1), origin).unwrap();
        }
        let no_op = operations
            .submit(&target_id, SetValue(1), OperationOrigin::LocalUser)
            .unwrap();
        assert_eq!(no_op.status, OperationApplyStatus::NoOp);
        assert!(!history.can_undo(&scope).unwrap());

        let transaction = operations
            .begin_transaction(OperationTransactionMode::New)
            .unwrap();
        operations
            .submit(&target_id, SetValue(2), OperationOrigin::LocalUser)
            .unwrap();
        operations
            .submit(&target_id, SetValue(3), OperationOrigin::LocalUser)
            .unwrap();
        transaction.commit().unwrap();

        assert!(history.can_undo(&scope).unwrap());
        assert!(history.undo(&scope).unwrap());
        assert_eq!(*target.0.lock().unwrap(), 1);
    }
}

mod fresh_replay {
    use std::sync::{Arc, Mutex};

    use rayx_runtime::services::{
        history::{HistoryScopeId, HistoryService},
        operations::{
            DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
            OperationTargetHandler, OperationTargetId,
        },
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct SetValue(i32);

    impl DomainOperation for SetValue {
        const KIND: &'static str = "history.fresh-replay.set";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct ValueTarget(Mutex<i32>);

    impl OperationTargetHandler<SetValue> for ValueTarget {
        fn apply_atomically(
            &self,
            operation: &SetValue,
            _: &OperationOrigin,
        ) -> Result<OperationApplyResult<SetValue>, String> {
            let mut value = self.0.lock().map_err(|_| "poisoned".to_string())?;
            let previous = *value;
            *value = operation.0;
            Ok(OperationApplyResult::applied(
                SetValue(previous),
                std::iter::empty::<String>(),
                previous.max(0) as u64,
                operation.0.max(0) as u64,
            ))
        }
    }
    #[test]
    fn replay_uses_the_inverse_captured_at_replay_time_for_the_opposite_stack() {
        let operations = OperationService::new();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("history:fresh-replay").unwrap();
        operations
            .register_target(target_id.clone(), target.clone())
            .unwrap();
        let history = HistoryService::new(operations.clone()).unwrap();
        let scope = HistoryScopeId::new("history:fresh-replay").unwrap();
        history
            .bind_target_scope(target_id.clone(), scope.clone())
            .unwrap();

        operations
            .submit(&target_id, SetValue(1), OperationOrigin::LocalUser)
            .unwrap();
        operations
            .submit(&target_id, SetValue(5), OperationOrigin::System)
            .unwrap();
        assert!(history.undo(&scope).unwrap());
        assert_eq!(*target.0.lock().unwrap(), 0);
        assert!(history.redo(&scope).unwrap());
        assert_eq!(
            *target.0.lock().unwrap(),
            5,
            "redo must use the inverse generated while undo replayed, not the original forward record"
        );
    }
}

mod multi_window {
    use std::sync::{Arc, Mutex};

    use rayx_runtime::services::{
        history::{HistoryScopeId, HistoryService},
        operations::{
            DomainOperation, OperationApplyResult, OperationOrigin, OperationService,
            OperationTargetHandler, OperationTargetId,
        },
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct SetValue(i32);

    impl DomainOperation for SetValue {
        const KIND: &'static str = "history.multi-window.set";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct ValueTarget(Mutex<i32>);

    impl OperationTargetHandler<SetValue> for ValueTarget {
        fn apply_atomically(
            &self,
            operation: &SetValue,
            _: &OperationOrigin,
        ) -> Result<OperationApplyResult<SetValue>, String> {
            let mut value = self.0.lock().map_err(|_| "poisoned".to_string())?;
            let previous = *value;
            *value = operation.0;
            Ok(OperationApplyResult::applied(
                SetValue(previous),
                std::iter::empty::<String>(),
                previous.max(0) as u64,
                operation.0.max(0) as u64,
            ))
        }
    }
    #[test]
    fn explicit_scopes_keep_two_windows_independent_without_global_focus() {
        let operations = OperationService::new();
        let left_id = OperationTargetId::new("history:window:left").unwrap();
        let right_id = OperationTargetId::new("history:window:right").unwrap();
        let left = Arc::new(ValueTarget::default());
        let right = Arc::new(ValueTarget::default());
        operations
            .register_target(left_id.clone(), left.clone())
            .unwrap();
        operations
            .register_target(right_id.clone(), right.clone())
            .unwrap();
        let history = HistoryService::new(operations.clone()).unwrap();
        let left_scope = HistoryScopeId::new("window:left").unwrap();
        let right_scope = HistoryScopeId::new("window:right").unwrap();
        let empty_scope = HistoryScopeId::new("window:empty").unwrap();
        history
            .bind_target_scope(left_id.clone(), left_scope.clone())
            .unwrap();
        history
            .bind_target_scope(right_id.clone(), right_scope.clone())
            .unwrap();

        operations
            .submit(&left_id, SetValue(1), OperationOrigin::LocalUser)
            .unwrap();
        operations
            .submit(&right_id, SetValue(2), OperationOrigin::LocalUser)
            .unwrap();
        assert!(history.can_undo(&left_scope).unwrap());
        assert!(history.can_undo(&right_scope).unwrap());
        assert!(!history.can_undo(&empty_scope).unwrap());
        assert!(!history.can_undo(&empty_scope).unwrap());

        assert!(history.undo(&left_scope).unwrap());
        assert_eq!(*left.0.lock().unwrap(), 0);
        assert_eq!(*right.0.lock().unwrap(), 2);
        assert!(history.can_undo(&right_scope).unwrap());
    }
}

mod journal_migration {
    use rayx_runtime::services::history::{
        HISTORY_JOURNAL_VERSION, HistoryJournalError, HistoryJournalV2, HistoryScopeId,
    };

    #[test]
    fn only_the_current_history_journal_version_is_accepted() {
        let scope = HistoryScopeId::new("history:migration").unwrap();
        let journal = HistoryJournalV2::new(3, scope.clone(), Vec::new());

        assert_eq!(journal.version, HISTORY_JOURNAL_VERSION);
        assert_eq!(
            HistoryJournalV2::decode(&journal.encode().unwrap()).unwrap(),
            journal
        );
        assert_eq!(
            HistoryJournalV2::decode(
                format!(
                    r#"{{"version":1,"revision":0,"scope_id":"{}","entries":[]}}"#,
                    scope.as_str()
                )
                .as_bytes(),
            )
            .unwrap_err(),
            HistoryJournalError::UnsupportedVersion
        );
        assert_eq!(
            HistoryJournalV2::decode(b"not a history journal").unwrap_err(),
            HistoryJournalError::Corrupt
        );
    }
}

mod legacy_history_reset {
    use rayx_runtime::services::history::{HistoryJournalError, HistoryJournalV2};

    #[test]
    fn legacy_snapshot_payload_is_rejected_as_opaque_data() {
        let legacy = br#"{
            "scope_id":"history:legacy",
            "undo_units":[{"apply_closure":"must never deserialize"}]
        }"#;
        let preserved = legacy.to_vec();

        assert_eq!(
            HistoryJournalV2::decode(legacy),
            Err(HistoryJournalError::Corrupt)
        );
        assert_eq!(preserved.as_slice(), legacy);
    }
}

mod registration {
    use rayx_core::RayXAppBuilder;
    use rayx_core::di::StartupInitializationContext;
    use rayx_runtime::services::history::{
        HistoryAppBuilderExt, HistoryService, InMemoryHistoryStore,
    };
    use rayx_runtime::services::operations::OperationService;

    fn start(builder: RayXAppBuilder) -> rayx_core::ServiceProvider {
        let services = builder.build().unwrap();
        futures::executor::block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .unwrap();
        services
    }

    #[test]
    fn add_history_registers_history_over_the_apps_operations() {
        let services = start(RayXAppBuilder::new().add_history());
        let history = services.required_service::<HistoryService>().unwrap();
        let operations = services.required_service::<OperationService>().unwrap();
        assert!(history.uses_operation_service(&operations));
    }

    #[test]
    fn add_history_with_store_persists_through_the_store() {
        let services =
            start(RayXAppBuilder::new().add_history_with_store(InMemoryHistoryStore::default()));
        let history = services.required_service::<HistoryService>().unwrap();
        let store = services
            .required_service::<dyn rayx_runtime::services::history::HistoryStore>()
            .unwrap();
        assert!(history.uses_history_store(&store));
    }
}
