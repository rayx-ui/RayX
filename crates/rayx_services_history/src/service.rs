//! RayX's history service.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak};

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceError,
    StartupServiceType,
};
use rayx_app::services::history::{
    HistoryEntry, HistoryError, HistoryEvent, HistoryNavigationFailure, HistoryScopeId,
    HistoryScopeState, HistoryService, HistoryState,
};
use rayx_app::services::operations::{
    OperationApplyStatus, OperationEvent, OperationOrigin, OperationReceipt, OperationRecord,
    OperationService, OperationTargetId, OperationTransactionMode,
};
use rayx_app::{ChangeSubscription, EventStream, StateCell};

use crate::store::HistoryStore;

/// How many failure codes the diagnostics keep.
#[cfg(feature = "diagnostics")]
pub(crate) const MAX_RECENT_FAILURES: usize = 32;

/// A scope's stacks and what replays and persistence need of it.
#[derive(Default)]
pub(crate) struct ScopeHistory {
    pub(crate) undo: Vec<HistoryEntry>,
    pub(crate) redo: Vec<HistoryEntry>,
    pub(crate) revision: u64,
    pub(crate) entry_limit: Option<usize>,
    pub(crate) replaying: bool,
    pub(crate) cleared_through_transaction: u64,
    pub(crate) persisted_revision: Option<u64>,
    pub(crate) loaded: bool,
}

impl ScopeHistory {
    pub(crate) fn enforce_entry_limit(&mut self) -> bool {
        let Some(limit) = self.entry_limit else {
            return false;
        };
        let excess = (self.undo.len() + self.redo.len()).saturating_sub(limit);
        let undo_excess = excess.min(self.undo.len());
        self.undo.drain(..undo_excess);
        self.redo.drain(..excess - undo_excess);
        excess != 0
    }

    fn published(&self) -> HistoryScopeState {
        HistoryScopeState {
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
            revision: self.revision,
            entry_limit: self.entry_limit,
        }
    }
}

/// What the service keeps.
#[derive(Default)]
pub(crate) struct Scopes {
    pub(crate) scopes: BTreeMap<HistoryScopeId, ScopeHistory>,
    target_scopes: BTreeMap<OperationTargetId, HistoryScopeId>,
    #[cfg(feature = "diagnostics")]
    pub(crate) recent_failure_codes: std::collections::VecDeque<String>,
    #[cfg(feature = "diagnostics")]
    unavailable_entry_count: usize,
}

/// RayX's history: records the local user's applied operations the app's operation service
/// publishes, per scope (a transaction is one entry), replays them through it, keeps each
/// scope's history in its store when it has one, and publishes the scopes' depths and each
/// step. Its commands answer at once when the operation service does (RayX's does), on any
/// thread.
pub(crate) struct RayxHistory {
    pub(crate) operations: Arc<dyn OperationService>,
    pub(crate) store: Mutex<Option<Arc<dyn HistoryStore>>>,
    store_configured: bool,
    pub(crate) scopes: Mutex<Scopes>,
    /// Held while a scope is loaded or saved.
    pub(crate) persistence: Mutex<()>,
    subscription: Mutex<Option<ChangeSubscription>>,
    history: StateCell<HistoryState>,
    events: EventStream<HistoryEvent>,
    /// Held while the state is published: a change made meanwhile (by a subscriber, or on
    /// another thread) marks it [`stale`](Self::stale) and the holder publishes again.
    publishing: Mutex<()>,
    stale: AtomicBool,
    initialization: StartupInitializationState,
}

impl RayxHistory {
    /// The history of `operations`' operations, kept in `store` when there is one; with
    /// `store_configured` and no store, the app's `dyn HistoryStore` from its initialization on.
    pub(crate) fn new(
        operations: Arc<dyn OperationService>,
        store: Option<Arc<dyn HistoryStore>>,
        store_configured: bool,
    ) -> Arc<Self> {
        let history = Arc::new(Self {
            operations: Arc::clone(&operations),
            store_configured: store_configured || store.is_some(),
            store: Mutex::new(store),
            scopes: Mutex::default(),
            persistence: Mutex::new(()),
            subscription: Mutex::new(None),
            history: StateCell::new(HistoryState::default()),
            events: EventStream::new(),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            initialization: StartupInitializationState::default(),
        });
        let observer: Weak<Self> = Arc::downgrade(&history);
        let subscription = operations.subscribe(move |event| {
            if let Some(history) = observer.upgrade() {
                history.observe_operation(event);
            }
        });
        *history
            .subscription
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(subscription);
        history
    }

    pub(crate) fn lock_state(&self) -> Result<MutexGuard<'_, Scopes>, HistoryError> {
        self.scopes.lock().map_err(|_| HistoryError::Poisoned)
    }

    /// Publishes the state now, unless it is the one published: a change made while it is
    /// published is published right after, by whoever is publishing.
    pub(crate) fn publish_state(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = {
                let scopes = self.scopes.lock().unwrap_or_else(PoisonError::into_inner);
                Self::snapshot(&scopes)
            };
            if *self.history.get() != state {
                self.history.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    fn snapshot(scopes: &Scopes) -> HistoryState {
        HistoryState {
            scopes: scopes
                .scopes
                .iter()
                .map(|(id, scope)| (id.clone(), scope.published()))
                .collect(),
            #[cfg(feature = "diagnostics")]
            unavailable_entry_count: scopes.unavailable_entry_count,
            #[cfg(feature = "diagnostics")]
            recent_failure_codes: scopes.recent_failure_codes.iter().cloned().collect(),
            #[cfg(not(feature = "diagnostics"))]
            unavailable_entry_count: 0,
            #[cfg(not(feature = "diagnostics"))]
            recent_failure_codes: Vec::new(),
        }
    }

    pub(crate) async fn bind_target_scope_now(
        &self,
        target_id: OperationTargetId,
        scope_id: HistoryScopeId,
    ) -> Result<(), HistoryError> {
        self.lock_state()?
            .target_scopes
            .insert(target_id, scope_id.clone());
        self.ensure_scope_loaded(&scope_id)
    }

    /// Undoes (`undo`) or redoes the scope's latest entry: its records resubmitted in a
    /// transaction of their own, recorded on the other stack with the inverses they answered.
    pub(crate) async fn replay(
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
        self.publish_state();
        self.persist_scope(scope_id)?;
        self.emit(if undo {
            HistoryEvent::UndoCompleted(scope_id.clone())
        } else {
            HistoryEvent::RedoCompleted(scope_id.clone())
        });
        Ok(true)
    }

    fn observe_operation(&self, event: &OperationEvent) {
        let OperationEvent::Committed(receipt) = event else {
            return;
        };
        if let Err(error) = self.record_receipt(receipt) {
            let scope_id = self
                .scopes
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
        if receipt.origin != OperationOrigin::LocalUser
            || receipt.status != OperationApplyStatus::Applied
        {
            return Ok(());
        }
        let scope_id = match self.lock_state()?.target_scopes.get(&receipt.target_id) {
            Some(scope_id) => scope_id.clone(),
            None => HistoryScopeId::new(receipt.target_id.as_str())?,
        };
        self.ensure_scope_loaded(&scope_id)?;

        let recorded = {
            let mut state = self.lock_state()?;
            let scope = state.scopes.entry(scope_id.clone()).or_default();
            if receipt.transaction_id <= scope.cleared_through_transaction {
                return Ok(());
            }
            let recorded = if let Some(entry) = scope
                .undo
                .last_mut()
                .filter(|entry| entry.transaction_id == receipt.transaction_id)
            {
                entry.forward.push(receipt.forward.clone());
                entry.inverse.push(inverse);
                entry.after_revision = receipt.after_revision;
                None
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
                Some(entry)
            };
            scope.revision += 1;
            scope.enforce_entry_limit();
            recorded
        };
        self.publish_state();
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
        let records: Vec<OperationRecord> = if undo {
            entry.inverse.iter().rev().cloned().collect()
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
        let (forward, inverse) = if undo {
            fresh_inverses.reverse();
            (fresh_inverses, records.into_iter().rev().collect())
        } else {
            (records, fresh_inverses)
        };
        Ok(HistoryEntry {
            transaction_id: entry.transaction_id,
            scope_id: entry.scope_id.clone(),
            target_id: entry.target_id.clone(),
            label_key: entry.label_key.clone(),
            forward,
            inverse,
            before_revision: entry.before_revision,
            after_revision: entry.after_revision,
        })
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
            {
                if matches!(
                    error,
                    HistoryError::Operation(
                        rayx_app::services::operations::OperationError::TargetReleased(_)
                    )
                ) {
                    state.unavailable_entry_count += 1;
                }
                state.recent_failure_codes.push_back(error.to_string());
                while state.recent_failure_codes.len() > MAX_RECENT_FAILURES {
                    state.recent_failure_codes.pop_front();
                }
            }
            if let Some(scope) = state.scopes.get_mut(&scope_id) {
                scope.revision += 1;
            }
        }
        self.publish_state();
        self.emit(HistoryEvent::NavigationFailed(HistoryNavigationFailure {
            scope_id,
            error_code: error.to_string(),
        }));
    }

    /// Publishes `event` (after the state it follows).
    pub(crate) fn emit(&self, event: HistoryEvent) {
        self.events.publish(event);
    }
}

/// A command's answer, boxed.
fn boxed<'a, T: Send + 'a>(
    call: impl std::future::Future<Output = T> + Send + 'a,
) -> BoxFuture<'a, T> {
    call.boxed()
}

impl HistoryService for RayxHistory {
    fn history(&self) -> &StateCell<HistoryState> {
        &self.history
    }

    fn events(&self) -> &EventStream<HistoryEvent> {
        &self.events
    }

    fn bind_target_scope(
        &self,
        target_id: OperationTargetId,
        scope_id: HistoryScopeId,
    ) -> BoxFuture<'_, Result<(), HistoryError>> {
        boxed(self.bind_target_scope_now(target_id, scope_id))
    }

    fn undo(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<bool, HistoryError>> {
        boxed(async move { self.replay(&scope_id, true).await })
    }

    fn redo(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<bool, HistoryError>> {
        boxed(async move { self.replay(&scope_id, false).await })
    }

    fn clear_scope(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<(), HistoryError>> {
        boxed(async move { self.clear_scope_now(&scope_id).await })
    }

    fn set_scope_entry_limit(
        &self,
        scope_id: HistoryScopeId,
        limit: usize,
    ) -> BoxFuture<'_, Result<(), HistoryError>> {
        boxed(async move { self.set_scope_entry_limit_now(&scope_id, limit).await })
    }

    fn retained_records(
        &self,
        scope_id: HistoryScopeId,
    ) -> BoxFuture<'_, Result<Vec<OperationRecord>, HistoryError>> {
        boxed(futures::future::ready(self.retained_records_now(&scope_id)))
    }
}

/// The history's startup role: with a configured store, it takes the app's `dyn HistoryStore`
/// once the store initialized.
pub(crate) struct Starting(pub(crate) Arc<RayxHistory>);

impl AsyncInitializable for Starting {
    fn is_initialized(&self) -> bool {
        self.0.initialization.is_initialized()
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let history = Arc::clone(&self.0);
        async move {
            if history.store_configured {
                let configured = context
                    .services()
                    .required_service::<dyn HistoryStore>()
                    .map_err(|error| StartupServiceError::new(error.to_string()))?;
                let mut store = history
                    .store
                    .lock()
                    .map_err(|_| StartupServiceError::new("history store state is poisoned"))?;
                if store
                    .as_ref()
                    .is_some_and(|existing| !Arc::ptr_eq(existing, &configured))
                {
                    return Err(StartupServiceError::new(
                        "the history's store is not the application's HistoryStore",
                    ));
                }
                *store = Some(configured);
            }
            history.initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Starting {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryService>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        let mut dependencies = vec![StartupServiceType::of::<dyn OperationService>()];
        if self.0.store_configured {
            dependencies.push(StartupServiceType::of::<dyn HistoryStore>());
        }
        dependencies
    }
}
