//! RayX's history service.

use std::collections::{BTreeMap, BTreeSet};
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
    OperationApplyStatus, OperationError, OperationEvent, OperationOrigin, OperationReceipt,
    OperationRecord, OperationService, OperationTargetId,
};
use rayx_app::{ChangeSubscription, EventStream, StateCell};

use crate::store::HistoryStore;
use crate::timer::HistoryTimer;
use std::time::Duration;

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
    /// The entries of the scope's journal in the store, as last loaded or saved (another
    /// history's included): what a save merges this history's entries into.
    pub(crate) stored: Vec<HistoryEntry>,
    /// The ids of this history's entries in that journal: an entry among them that is no
    /// longer on the undo stack was dropped here (undone, cleared, evicted).
    pub(crate) saved_own: BTreeSet<String>,
    /// How many entries were recorded in it (pushed, not merged): what a replay tells an edit
    /// recorded meanwhile by.
    pub(crate) pushes: u64,
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
    /// The scopes changed since their last save.
    pub(crate) unsaved: BTreeSet<HistoryScopeId>,
    /// Whether a writer is saving them.
    pub(crate) writing: bool,
    /// The commands waiting for a scope's next save.
    pub(crate) save_waiters:
        BTreeMap<HistoryScopeId, Vec<futures::channel::oneshot::Sender<Result<(), HistoryError>>>>,
    #[cfg(feature = "diagnostics")]
    pub(crate) recent_failure_codes: std::collections::VecDeque<String>,
    #[cfg(feature = "diagnostics")]
    unavailable_entry_count: usize,
}

/// RayX's history: records the local user's applied operations the app's operation service
/// publishes, per scope (a transaction is one entry), replays them through it (in a transaction
/// of its own, carried by id), keeps each scope's history in its store when it has one, and
/// publishes the scopes' depths and each step. Its commands answer at once when the operation
/// service and the store do (RayX's do), on any thread; with services answering later (TS
/// implementations), its commands await them and its saves follow its changes.
pub(crate) struct RayxHistory {
    /// Itself, for the work it runs detached.
    pub(crate) this: Weak<Self>,
    /// What this history's entry ids start with: unique to it, across app runs.
    session: String,
    next_entry: std::sync::atomic::AtomicU64,
    /// Bounds each call of the operation service and the store (`None`: unbounded).
    timer: Option<HistoryTimer>,
    timeout: Option<Duration>,
    pub(crate) operations: Arc<dyn OperationService>,
    pub(crate) store: Mutex<Option<Arc<dyn HistoryStore>>>,
    pub(crate) store_configured: bool,
    pub(crate) scopes: Mutex<Scopes>,
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
        timer: Option<HistoryTimer>,
        timeout: Option<Duration>,
    ) -> Arc<Self> {
        let history = Arc::new_cyclic(|this| Self {
            this: this.clone(),
            session: session_id(),
            next_entry: std::sync::atomic::AtomicU64::new(1),
            timer,
            timeout,
            operations: Arc::clone(&operations),
            store_configured: store_configured || store.is_some(),
            store: Mutex::new(store),
            scopes: Mutex::default(),
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

    /// A new entry id.
    fn next_entry_id(&self) -> String {
        let number = self.next_entry.fetch_add(1, Ordering::Relaxed);
        format!("{}:{number}", self.session)
    }

    pub(crate) fn lock_state(&self) -> Result<MutexGuard<'_, Scopes>, HistoryError> {
        self.scopes.lock().map_err(|_| HistoryError::Poisoned)
    }

    /// What `call` gives, unless it takes longer than the history's timeout
    /// ([`HistoryError::TimedOut`]).
    pub(crate) async fn bounded_store<T>(
        &self,
        call: impl std::future::Future<Output = T>,
    ) -> Result<T, HistoryError> {
        let (Some(timer), Some(timeout)) = (&self.timer, self.timeout) else {
            return Ok(call.await);
        };
        let call = std::pin::pin!(call);
        match futures::future::select(call, timer.after(timeout)).await {
            futures::future::Either::Left((value, _)) => Ok(value),
            futures::future::Either::Right(_) => Err(HistoryError::TimedOut),
        }
    }

    /// A call of the operation service or the store, bounded by the history's timeout.
    pub(crate) async fn bounded<T, E: Into<HistoryError>>(
        &self,
        call: impl std::future::Future<Output = Result<T, E>>,
    ) -> Result<T, HistoryError> {
        self.bounded_store(call).await?.map_err(Into::into)
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
        self.ensure_scope_loaded(&scope_id).await
    }

    /// Undoes (`undo`) or redoes the scope's latest entry: its records resubmitted in a
    /// transaction of their own, recorded on the other stack with the inverses they answered.
    /// The scope is busy meanwhile (until the replay ends, fails or is dropped); an operation
    /// recorded meanwhile is kept, the replayed entry removed by identity.
    pub(crate) async fn replay(
        &self,
        scope_id: &HistoryScopeId,
        undo: bool,
    ) -> Result<bool, HistoryError> {
        self.ensure_scope_loaded(scope_id).await?;
        let Some(entry) = self.peek_entry(scope_id, undo)? else {
            return Ok(false);
        };
        let mut replaying = Replaying {
            history: self,
            scope_id: Some(scope_id),
        };
        let (replayed, pushes_at_commit) = match self.resubmit(scope_id, &entry, undo).await {
            Ok(replayed) => replayed,
            Err(error) => {
                drop(replaying);
                self.record_failure(scope_id.clone(), &error);
                return Err(error);
            }
        };
        {
            let mut state = self.lock_state()?;
            let scope = state.scopes.entry(scope_id.clone()).or_default();
            scope.replaying = false;
            replaying.scope_id = None;
            let from = if undo {
                &mut scope.undo
            } else {
                &mut scope.redo
            };
            // Gone from its stack only if it changed between the check before the commit and
            // the commit: the change is committed, so it is recorded all the same.
            if let Some(position) = from.iter().rposition(|candidate| *candidate == entry) {
                from.remove(position);
            }
            if undo {
                // An edit recorded while the commit was pending came after the undo's change:
                // it cleared the redo stack, so the undone entry is not kept there.
                if scope.pushes == pushes_at_commit {
                    scope.redo.push(replayed);
                }
            } else {
                // Below the edits recorded while the commit was pending, which came after it.
                let above = usize::try_from(scope.pushes - pushes_at_commit).unwrap_or(usize::MAX);
                let at = scope.undo.len().saturating_sub(above);
                scope.undo.insert(at, replayed);
            }
            scope.revision += 1;
        }
        self.publish_state();
        self.persist_scope(scope_id).await?;
        self.emit(if undo {
            HistoryEvent::UndoCompleted(scope_id.clone())
        } else {
            HistoryEvent::RedoCompleted(scope_id.clone())
        });
        Ok(true)
    }

    fn observe_operation(self: &Arc<Self>, event: &OperationEvent) {
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

    /// Records `receipt` (synchronously: it is heard inside the operation service's publish),
    /// then saves its scope behind it, without waiting.
    fn record_receipt(self: &Arc<Self>, receipt: &OperationReceipt) -> Result<(), HistoryError> {
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
                    id: self.next_entry_id(),
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
                scope.pushes += 1;
                scope.redo.clear();
                Some(entry)
            };
            scope.revision += 1;
            scope.enforce_entry_limit();
            recorded
        };
        self.publish_state();
        self.persist_behind(&scope_id);
        if let Some(entry) = recorded {
            self.emit(HistoryEvent::Recorded(entry));
        }
        Ok(())
    }

    /// Resubmits `entry`'s records in a transaction of its own, committed only while the entry
    /// is still on its stack of `scope_id` (an edit recorded meanwhile may have cleared the redo
    /// stack): else aborted, and `StackChanged`.
    async fn resubmit(
        &self,
        scope_id: &HistoryScopeId,
        entry: &HistoryEntry,
        undo: bool,
    ) -> Result<(HistoryEntry, u64), HistoryError> {
        let transaction = OpenTransaction {
            operations: &*self.operations,
            id: Some(self.begin_bounded().await?),
        };
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
                .bounded(
                    self.operations
                        .submit_record(record, origin.clone(), transaction.id),
                )
                .await
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    let _ = self.bounded(transaction.abort()).await;
                    return Err(error);
                }
            };
            if receipt.status != OperationApplyStatus::Applied {
                let _ = self.bounded(transaction.abort()).await;
                return Err(HistoryError::ApplyStatus(receipt.status));
            }
            receipts.push(receipt);
        }
        let (still_there, pushes_at_commit) = {
            let state = self.lock_state()?;
            let scope = state.scopes.get(scope_id);
            (
                scope.is_some_and(|scope| {
                    let from = if undo { &scope.undo } else { &scope.redo };
                    from.contains(entry)
                }),
                scope.map_or(0, |scope| scope.pushes),
            )
        };
        if !still_there {
            let _ = self.bounded(transaction.abort()).await;
            return Err(HistoryError::StackChanged);
        }
        // Not bounded: a commit that answers late still lands, and the stacks must follow it, so
        // the scope stays busy until it answers.
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
        let replayed = HistoryEntry {
            id: entry.id.clone(),
            transaction_id: entry.transaction_id,
            scope_id: entry.scope_id.clone(),
            target_id: entry.target_id.clone(),
            label_key: entry.label_key.clone(),
            forward,
            inverse,
            before_revision: entry.before_revision,
            after_revision: entry.after_revision,
        };
        Ok((replayed, pushes_at_commit))
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

    pub(crate) fn record_failure(&self, scope_id: HistoryScopeId, error: &HistoryError) {
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

/// A scope being replayed: dropped before the replay finished (it failed, or its caller gave
/// up on it), it frees the scope.
struct Replaying<'a> {
    history: &'a RayxHistory,
    scope_id: Option<&'a HistoryScopeId>,
}

impl Drop for Replaying<'_> {
    fn drop(&mut self) {
        if let Some(scope_id) = self.scope_id {
            let mut state = self
                .history
                .scopes
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(scope) = state.scopes.get_mut(scope_id) {
                scope.replaying = false;
            }
        }
    }
}

/// A transaction the history opened, by id: committed or aborted explicitly, and aborted (at
/// once when the operation service answers at once, else started) when dropped open.
pub(crate) struct OpenTransaction<'a> {
    pub(crate) operations: &'a dyn OperationService,
    pub(crate) id: Option<u64>,
}

impl OpenTransaction<'_> {
    pub(crate) async fn commit(mut self) -> Result<(), HistoryError> {
        match self.id.take() {
            Some(id) => Ok(self.operations.commit(id).await?),
            None => Ok(()),
        }
    }

    pub(crate) async fn abort(mut self) -> Result<(), HistoryError> {
        match self.id.take() {
            Some(id) => Ok(self.operations.abort(id).await?),
            None => Ok(()),
        }
    }
}

impl Drop for OpenTransaction<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let _ = self.operations.abort(id).now_or_never();
        }
    }
}

impl RayxHistory {
    /// Runs `command` detached, started now: the returned future awaits only its answer, so
    /// dropping it never leaves a replay, a clear or a save half done.
    /// A failure its caller no longer waits for is a failed step of `scope_id`.
    fn detached<T, F>(
        &self,
        scope_id: &HistoryScopeId,
        command: impl FnOnce(Arc<Self>) -> F,
    ) -> BoxFuture<'static, Result<T, HistoryError>>
    where
        T: Send + 'static,
        F: std::future::Future<Output = Result<T, HistoryError>> + Send + 'static,
    {
        let Some(history) = self.this.upgrade() else {
            return futures::future::ready(Err(HistoryError::Poisoned)).boxed();
        };
        let (answer, answered) = futures::channel::oneshot::channel();
        let work = command(Arc::clone(&history));
        let answer_scope = scope_id.clone();
        let reporter = Arc::downgrade(&history);
        crate::detached::spawn(Box::pin(async move {
            if let Err(Err(error)) = answer.send(work.await) {
                history.record_failure(answer_scope, &error);
            }
        }));
        Answer {
            answered,
            history: reporter,
            scope_id: scope_id.clone(),
            read: false,
        }
        .boxed()
    }

    /// Begins a transaction, within the timeout: one whose id arrives after it is aborted
    /// then, so no transaction is left open.
    pub(crate) async fn begin_bounded(&self) -> Result<u64, HistoryError> {
        let operations = Arc::clone(&self.operations);
        let begin: BoxFuture<'static, Result<u64, OperationError>> =
            async move { operations.begin().await }.boxed();
        let (Some(timer), Some(timeout)) = (&self.timer, self.timeout) else {
            return Ok(begin.await?);
        };
        match futures::future::select(begin, timer.after(timeout)).await {
            futures::future::Either::Left((begun, _)) => Ok(begun?),
            futures::future::Either::Right((_, begin)) => {
                let operations = Arc::clone(&self.operations);
                crate::detached::spawn(Box::pin(async move {
                    if let Ok(late) = begin.await {
                        let _ = operations.abort(late).await;
                    }
                }));
                Err(HistoryError::TimedOut)
            }
        }
    }
}

/// A detached command's answer: an error the caller drops unread (it gave up on the command
/// after the command finished) is a failed step of the scope.
struct Answer<T> {
    answered: futures::channel::oneshot::Receiver<Result<T, HistoryError>>,
    history: Weak<RayxHistory>,
    scope_id: HistoryScopeId,
    read: bool,
}

impl<T> std::future::Future for Answer<T> {
    type Output = Result<T, HistoryError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let answer = std::task::ready!(self.answered.poll_unpin(cx));
        self.read = true;
        std::task::Poll::Ready(answer.unwrap_or(Err(HistoryError::Poisoned)))
    }
}

impl<T> Drop for Answer<T> {
    fn drop(&mut self) {
        if self.read {
            return;
        }
        if let Ok(Some(Err(error))) = self.answered.try_recv()
            && let Some(history) = self.history.upgrade()
        {
            history.record_failure(self.scope_id.clone(), &error);
        }
    }
}

/// What a history's entry ids start with: the wall clock, the process's random hash keys and a
/// count of the histories made, so two histories (in one run or two) never share it.
fn session_id() -> String {
    use std::hash::{BuildHasher as _, Hasher as _};
    static HISTORIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let made = HISTORIES.fetch_add(1, Ordering::Relaxed);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(nsh_core::time::unix_ms().to_bits());
    #[cfg(not(target_arch = "wasm32"))]
    hasher.write_u32(std::process::id());
    hasher.write_u64(made);
    format!("{:016x}{:x}", hasher.finish(), made)
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
        self.detached(&scope_id.clone(), move |history| async move {
            history.replay(&scope_id, true).await
        })
    }

    fn redo(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<bool, HistoryError>> {
        self.detached(&scope_id.clone(), move |history| async move {
            history.replay(&scope_id, false).await
        })
    }

    fn clear_scope(&self, scope_id: HistoryScopeId) -> BoxFuture<'_, Result<(), HistoryError>> {
        self.detached(&scope_id.clone(), move |history| async move {
            history.clear_scope_now(&scope_id).await
        })
    }

    fn set_scope_entry_limit(
        &self,
        scope_id: HistoryScopeId,
        limit: usize,
    ) -> BoxFuture<'_, Result<(), HistoryError>> {
        self.detached(&scope_id.clone(), move |history| async move {
            history.set_scope_entry_limit_now(&scope_id, limit).await
        })
    }

    fn retained_records(
        &self,
        scope_id: HistoryScopeId,
    ) -> BoxFuture<'_, Result<Vec<OperationRecord>, HistoryError>> {
        boxed(async move { self.retained_records_now(&scope_id).await })
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
                drop(store);
                // What was recorded before the store was attached is saved now.
                history.save_unsaved();
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
