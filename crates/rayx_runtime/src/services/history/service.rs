use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, Weak},
};

#[cfg(feature = "diagnostics")]
use super::HistoryDiagnostics;
use super::{
    HistoryEntry, HistoryError, HistoryEvent, HistoryNavigationFailure, HistoryScopeId,
    HistoryStore,
};
use crate::services::operations::{
    OperationApplyStatus, OperationError, OperationEvent, OperationOrigin, OperationReceipt,
    OperationService, OperationTargetId, OperationTransactionMode,
};
use futures::{FutureExt, future::LocalBoxFuture};
use rayx_app::ChangeSubscription;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use rayx_app::services::ContractFailure;

#[cfg(feature = "diagnostics")]
pub(super) const MAX_RECENT_FAILURES: usize = 32;

type HistorySubscriber = Arc<dyn Fn(&HistoryEvent) + Send + Sync>;

#[derive(Default)]
pub(super) struct ScopeHistory {
    pub(super) undo: Vec<HistoryEntry>,
    pub(super) redo: Vec<HistoryEntry>,
    pub(super) revision: u64,
    pub(super) entry_limit: Option<usize>,
    pub(super) replaying: bool,
    pub(super) cleared_through_transaction: u64,
    pub(super) persisted_revision: Option<u64>,
    pub(super) loaded: bool,
}

impl ScopeHistory {
    pub(super) fn enforce_entry_limit(&mut self) -> bool {
        let Some(limit) = self.entry_limit else {
            return false;
        };
        let excess = (self.undo.len() + self.redo.len()).saturating_sub(limit);
        let undo_excess = excess.min(self.undo.len());
        self.undo.drain(..undo_excess);
        self.redo.drain(..excess - undo_excess);
        excess != 0
    }
}

#[derive(Default)]
pub(super) struct HistoryState {
    pub(super) scopes: BTreeMap<HistoryScopeId, ScopeHistory>,
    target_scopes: BTreeMap<OperationTargetId, HistoryScopeId>,
    subscribers: BTreeMap<u64, HistorySubscriber>,
    next_subscriber_id: u64,
    #[cfg(feature = "diagnostics")]
    pub(super) recent_failure_codes: std::collections::VecDeque<String>,
    #[cfg(feature = "diagnostics")]
    unavailable_entry_count: usize,
}

#[derive(Clone)]
pub struct HistoryService {
    pub(super) operations: Arc<dyn OperationService>,
    pub(super) store: Arc<Mutex<Option<Arc<dyn HistoryStore>>>>,
    store_configured: bool,
    pub(super) state: Arc<Mutex<HistoryState>>,
    operation_subscription: Arc<Mutex<Option<ChangeSubscription>>>,
    pub(super) persistence: Arc<Mutex<()>>,
    initialization: StartupInitializationState,
}

pub struct HistorySubscription {
    state: Weak<Mutex<HistoryState>>,
    id: u64,
}

impl Drop for HistorySubscription {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .subscribers
                .remove(&self.id);
        }
    }
}

impl HistoryService {
    pub fn new(operations: Arc<dyn OperationService>) -> Result<Self, HistoryError> {
        Self::create(operations, None, false)
    }

    pub fn with_store(
        operations: Arc<dyn OperationService>,
        store: Arc<dyn HistoryStore>,
    ) -> Result<Self, HistoryError> {
        Self::create(operations, Some(store), true)
    }

    pub fn with_configured_store(
        operations: Arc<dyn OperationService>,
    ) -> Result<Self, HistoryError> {
        Self::create(operations, None, true)
    }

    fn create(
        operations: Arc<dyn OperationService>,
        store: Option<Arc<dyn HistoryStore>>,
        store_configured: bool,
    ) -> Result<Self, HistoryError> {
        let service = Self {
            operations: Arc::clone(&operations),
            store: Arc::new(Mutex::new(store)),
            store_configured,
            state: Arc::new(Mutex::new(HistoryState::default())),
            operation_subscription: Arc::new(Mutex::new(None)),
            persistence: Arc::new(Mutex::new(())),
            initialization: StartupInitializationState::default(),
        };
        let observer = service.clone();
        let subscription = operations.subscribe(move |event| observer.observe_operation(event));
        *service
            .operation_subscription
            .lock()
            .map_err(|_| HistoryError::Poisoned)? = Some(subscription);
        Ok(service)
    }

    /// Reports whether this history service observes the exact operation-service instance.
    pub fn uses_operation_service(&self, operations: &dyn OperationService) -> bool {
        self.operations.instance_id() == operations.instance_id()
    }

    pub fn uses_history_store(&self, store: &Arc<dyn HistoryStore>) -> bool {
        self.store
            .lock()
            .ok()
            .and_then(|configured| configured.clone())
            .is_some_and(|configured| Arc::ptr_eq(&configured, store))
    }

    pub fn bind_target_scope(
        &self,
        target_id: OperationTargetId,
        scope_id: HistoryScopeId,
    ) -> Result<(), HistoryError> {
        self.lock_state()?.target_scopes.insert(target_id, scope_id);
        Ok(())
    }

    pub fn can_undo(&self, scope_id: &HistoryScopeId) -> Result<bool, HistoryError> {
        self.ensure_scope_loaded(scope_id)?;
        Ok(self
            .lock_state()?
            .scopes
            .get(scope_id)
            .is_some_and(|scope| !scope.undo.is_empty()))
    }

    pub fn can_redo(&self, scope_id: &HistoryScopeId) -> Result<bool, HistoryError> {
        self.ensure_scope_loaded(scope_id)?;
        Ok(self
            .lock_state()?
            .scopes
            .get(scope_id)
            .is_some_and(|scope| !scope.redo.is_empty()))
    }

    pub fn undo(&self, scope_id: &HistoryScopeId) -> Result<bool, HistoryError> {
        at_once(self.replay(scope_id, true), "undo")
    }

    pub fn redo(&self, scope_id: &HistoryScopeId) -> Result<bool, HistoryError> {
        at_once(self.replay(scope_id, false), "redo")
    }

    /// Undoes (`undo`) or redoes the scope's latest entry: its records resubmitted in a
    /// transaction of their own, recorded on the other stack with the inverses they answered.
    pub(super) async fn replay(
        &self,
        scope_id: &HistoryScopeId,
        undo: bool,
    ) -> Result<bool, HistoryError> {
        self.ensure_scope_loaded(scope_id)?;
        let Some(entry) = self.peek_entry(scope_id, undo)? else {
            return Ok(false);
        };
        let replayed = match self.resubmit(&entry, undo).await {
            Ok(entry) => entry,
            Err(error) => {
                if let Some(scope) = self.lock_state()?.scopes.get_mut(scope_id) {
                    scope.replaying = false;
                }
                self.record_failure(scope_id.clone(), &error);
                return Err(error);
            }
        };
        {
            let mut state = self.lock_state()?;
            let scope = state.scopes.entry(scope_id.clone()).or_default();
            let (from, to) = if undo {
                (&mut scope.undo, &mut scope.redo)
            } else {
                (&mut scope.redo, &mut scope.undo)
            };
            from.pop().ok_or(HistoryError::StackChanged)?;
            to.push(replayed);
            scope.replaying = false;
            scope.revision += 1;
        }
        self.persist_scope(scope_id)?;
        self.emit(if undo {
            HistoryEvent::UndoCompleted(scope_id.clone())
        } else {
            HistoryEvent::RedoCompleted(scope_id.clone())
        });
        Ok(true)
    }

    pub fn subscribe(
        &self,
        subscriber: impl Fn(&HistoryEvent) + Send + Sync + 'static,
    ) -> Result<HistorySubscription, HistoryError> {
        let mut state = self.lock_state()?;
        state.next_subscriber_id += 1;
        let id = state.next_subscriber_id;
        state.subscribers.insert(id, Arc::new(subscriber));
        Ok(HistorySubscription {
            state: Arc::downgrade(&self.state),
            id,
        })
    }

    #[cfg(feature = "diagnostics")]
    pub fn diagnostics(&self, scope_id: &HistoryScopeId) -> HistoryDiagnostics {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (undo_depth, redo_depth, stack_revision) =
            state.scopes.get(scope_id).map_or((0, 0, 0), |scope| {
                (scope.undo.len(), scope.redo.len(), scope.revision)
            });
        HistoryDiagnostics {
            undo_depth,
            redo_depth,
            stack_revision,
            subscriber_count: state.subscribers.len(),
            unavailable_entry_count: state.unavailable_entry_count,
            recent_failure_codes: state.recent_failure_codes.iter().cloned().collect(),
        }
    }

    fn observe_operation(&self, event: &OperationEvent) {
        let OperationEvent::Committed(receipt) = event else {
            return;
        };
        if let Err(error) = self.record_receipt(receipt) {
            let scope_id = self
                .state
                .lock()
                .ok()
                .and_then(|state| state.target_scopes.get(&receipt.target_id).cloned())
                .or_else(|| HistoryScopeId::new(receipt.target_id.as_str()).ok());
            if let Some(scope_id) = scope_id {
                self.record_failure(scope_id, &error);
            }
        }
    }

    fn record_receipt(&self, receipt: &OperationReceipt) -> Result<(), HistoryError> {
        let Some(inverse) = receipt.inverse.clone() else {
            return Ok(());
        };
        let state = self.lock_state()?;
        if receipt.origin != OperationOrigin::LocalUser
            || receipt.status != OperationApplyStatus::Applied
        {
            return Ok(());
        }
        let scope_id = match state.target_scopes.get(&receipt.target_id) {
            Some(scope_id) => scope_id.clone(),
            None => HistoryScopeId::new(receipt.target_id.as_str())?,
        };
        drop(state);
        self.ensure_scope_loaded(&scope_id)?;

        let mut state = self.lock_state()?;
        let scope = state.scopes.entry(scope_id.clone()).or_default();
        let mut recorded = None;
        if receipt.transaction_id <= scope.cleared_through_transaction {
            return Ok(());
        }
        if scope
            .undo
            .last()
            .is_some_and(|entry| entry.transaction_id == receipt.transaction_id)
        {
            let entry = scope.undo.last_mut().ok_or(HistoryError::StackChanged)?;
            entry.forward.push(receipt.forward.clone());
            entry.inverse.push(inverse);
            entry.after_revision = receipt.after_revision;
            scope.revision += 1;
        } else {
            let entry = HistoryEntry {
                transaction_id: receipt.transaction_id,
                scope_id: scope_id.clone(),
                target_id: receipt.target_id.clone(),
                label_key: receipt.forward.kind.clone(),
                forward: vec![receipt.forward.clone()],
                inverse: vec![inverse],
                before_revision: receipt.before_revision,
                after_revision: receipt.after_revision,
            };
            scope.undo.push(entry.clone());
            scope.redo.clear();
            scope.revision += 1;
            recorded = Some(entry);
        }
        scope.enforce_entry_limit();
        drop(state);
        self.persist_scope(&scope_id)?;
        if let Some(entry) = recorded {
            self.emit(HistoryEvent::Recorded(entry));
        }
        Ok(())
    }

    async fn resubmit(
        &self,
        entry: &HistoryEntry,
        undo: bool,
    ) -> Result<HistoryEntry, HistoryError> {
        let transaction = self
            .operations
            .begin_transaction(OperationTransactionMode::New)
            .await?;
        let records = if undo {
            entry.inverse.iter().rev().cloned().collect::<Vec<_>>()
        } else {
            entry.forward.clone()
        };
        let origin = if undo {
            OperationOrigin::HistoryUndo
        } else {
            OperationOrigin::HistoryRedo
        };
        let mut receipts = Vec::with_capacity(records.len());
        for record in records.iter().cloned() {
            let receipt = match self
                .operations
                .submit_record(record, origin.clone(), Some(transaction.id()))
                .await
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    let _ = transaction.abort().await;
                    return Err(error.into());
                }
            };
            if receipt.status != OperationApplyStatus::Applied {
                let _ = transaction.abort().await;
                return Err(HistoryError::ApplyStatus(receipt.status));
            }
            receipts.push(receipt);
        }
        transaction.commit().await?;
        let mut fresh_inverses = receipts
            .into_iter()
            .map(|receipt| receipt.inverse.ok_or(HistoryError::StackChanged))
            .collect::<Result<Vec<_>, _>>()?;
        if undo {
            fresh_inverses.reverse();
            Ok(HistoryEntry {
                transaction_id: entry.transaction_id,
                scope_id: entry.scope_id.clone(),
                target_id: entry.target_id.clone(),
                label_key: entry.label_key.clone(),
                forward: fresh_inverses,
                inverse: records.into_iter().rev().collect(),
                before_revision: entry.before_revision,
                after_revision: entry.after_revision,
            })
        } else {
            Ok(HistoryEntry {
                transaction_id: entry.transaction_id,
                scope_id: entry.scope_id.clone(),
                target_id: entry.target_id.clone(),
                label_key: entry.label_key.clone(),
                forward: records,
                inverse: fresh_inverses,
                before_revision: entry.before_revision,
                after_revision: entry.after_revision,
            })
        }
    }

    fn peek_entry(
        &self,
        scope_id: &HistoryScopeId,
        undo: bool,
    ) -> Result<Option<HistoryEntry>, HistoryError> {
        let mut state = self.lock_state()?;
        let Some(scope) = state.scopes.get_mut(scope_id) else {
            return Ok(None);
        };
        if scope.replaying {
            return Err(HistoryError::ScopeBusy);
        }
        let entry = if undo {
            scope.undo.last().cloned()
        } else {
            scope.redo.last().cloned()
        };
        scope.replaying = entry.is_some();
        Ok(entry)
    }

    fn record_failure(&self, scope_id: HistoryScopeId, error: &HistoryError) {
        if let Ok(mut state) = self.lock_state() {
            #[cfg(feature = "diagnostics")]
            if matches!(
                error,
                HistoryError::Operation(
                    crate::services::operations::OperationError::TargetReleased(_)
                )
            ) {
                state.unavailable_entry_count += 1;
            }
            #[cfg(feature = "diagnostics")]
            state.recent_failure_codes.push_back(error.to_string());
            #[cfg(feature = "diagnostics")]
            while state.recent_failure_codes.len() > MAX_RECENT_FAILURES {
                state.recent_failure_codes.pop_front();
            }
            if let Some(scope) = state.scopes.get_mut(&scope_id) {
                scope.revision += 1;
            }
        }
        self.emit(HistoryEvent::NavigationFailed(HistoryNavigationFailure {
            scope_id,
            error_code: error.to_string(),
        }));
    }

    pub(super) fn emit(&self, event: HistoryEvent) {
        let subscribers = self
            .state
            .lock()
            .map(|state| state.subscribers.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for subscriber in subscribers {
            let _ = catch_unwind(AssertUnwindSafe(|| subscriber(&event)));
        }
    }

    pub(super) fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HistoryState>, HistoryError> {
        self.state.lock().map_err(|_| HistoryError::Poisoned)
    }
}

impl AsyncInitializable for HistoryService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialization = self.initialization.clone();
        let store = self.store.clone();
        let store_configured = self.store_configured;
        async move {
            if store_configured {
                let configured = context
                    .services()
                    .required_service::<dyn HistoryStore>()
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
                let mut store = store
                    .lock()
                    .map_err(|_| StartupServiceError::new("history store state is poisoned"))?;
                if store
                    .as_ref()
                    .is_some_and(|existing| !Arc::ptr_eq(existing, &configured))
                {
                    return Err(StartupServiceError::new(
                        "HistoryService store does not match the application HistoryStore",
                    ));
                }
                *store = Some(configured);
            }
            initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for HistoryService {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<Self>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        let mut dependencies = vec![StartupServiceType::of::<dyn OperationService>()];
        if self.store_configured {
            dependencies.push(StartupServiceType::of::<dyn HistoryStore>());
        }
        dependencies
    }
}

/// What `call` answered at once: RayX's history needs an operation service that answers at once
/// (as RayX's does) for its synchronous calls.
pub(super) fn at_once<T>(
    call: impl std::future::Future<Output = Result<T, HistoryError>>,
    member: &'static str,
) -> Result<T, HistoryError> {
    call.now_or_never().unwrap_or_else(|| {
        Err(HistoryError::Operation(OperationError::Failed(
            ContractFailure {
                contract: "OperationService",
                member,
                message: "the operation service answers later; the history's synchronous calls \
                          need one that answers at once"
                    .to_owned(),
            },
        )))
    })
}
