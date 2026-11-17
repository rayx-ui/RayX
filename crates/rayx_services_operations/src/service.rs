//! RayX's operation service.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_app::services::operations::{
    OperationApplyStatus, OperationError, OperationEvent, OperationFailure, OperationOrigin,
    OperationReceipt, OperationRecord, OperationService, OperationState, OperationTargetHandle,
    OperationTargetId,
};
use rayx_app::{EventStream, StateCell};

/// How many committed operations and failure codes the diagnostics keep.
#[cfg(feature = "diagnostics")]
const MAX_RECENT_TRANSACTIONS: usize = 32;

/// What the service keeps.
#[derive(Default)]
struct Targets {
    registered: BTreeMap<OperationTargetId, RegisteredTarget>,
    released: BTreeSet<OperationTargetId>,
    /// The open transactions' receipts, with the handler that applied each.
    pending: BTreeMap<u64, Vec<(OperationReceipt, OperationTargetHandle)>>,
    #[cfg(feature = "diagnostics")]
    recent_transactions:
        std::collections::VecDeque<rayx_app::services::operations::OperationTransactionDiagnostics>,
    #[cfg(feature = "diagnostics")]
    recent_failure_codes: std::collections::VecDeque<String>,
}

/// A registered target: its handler, and the gate its operations apply through one at a time.
#[derive(Clone)]
struct RegisteredTarget {
    handler: OperationTargetHandle,
    gate: std::sync::Arc<Mutex<()>>,
}

/// RayX's operation service: applies each record through its target's handler (one operation
/// of a target at a time), publishes the receipt (at once, or when its transaction commits)
/// and the registered targets. Its commands answer at once, on any thread.
pub(crate) struct RayxOperations {
    instance_id: u64,
    next_transaction: AtomicU64,
    targets: Mutex<Targets>,
    operations: StateCell<OperationState>,
    events: EventStream<OperationEvent>,
    /// Held while the state is published: a change made meanwhile (by a subscriber, or on
    /// another thread) marks it [`stale`](Self::stale) and the holder publishes again.
    publishing: Mutex<()>,
    stale: AtomicBool,
}

impl RayxOperations {
    pub(crate) fn new() -> Self {
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
        Self {
            instance_id: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            next_transaction: AtomicU64::new(0),
            targets: Mutex::default(),
            operations: StateCell::new(OperationState::default()),
            events: EventStream::new(),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Targets>, OperationError> {
        self.targets.lock().map_err(|_| OperationError::Poisoned)
    }

    fn next_transaction_id(&self) -> u64 {
        self.next_transaction.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Publishes the state now, unless it is the one published: a change made while it is
    /// published is published right after, by whoever is publishing.
    fn publish_state(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let state = {
                let targets = self.targets.lock().unwrap_or_else(PoisonError::into_inner);
                Self::snapshot(&targets)
            };
            if *self.operations.get() != state {
                self.operations.publish(state);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    fn snapshot(targets: &Targets) -> OperationState {
        OperationState {
            targets: targets.registered.keys().cloned().collect(),
            #[cfg(feature = "diagnostics")]
            recent_transactions: targets.recent_transactions.iter().cloned().collect(),
            #[cfg(feature = "diagnostics")]
            recent_failure_codes: targets.recent_failure_codes.iter().cloned().collect(),
            #[cfg(not(feature = "diagnostics"))]
            recent_transactions: Vec::new(),
            #[cfg(not(feature = "diagnostics"))]
            recent_failure_codes: Vec::new(),
        }
    }

    fn missing(targets: &Targets, target_id: &OperationTargetId) -> OperationError {
        if targets.released.contains(target_id) {
            OperationError::TargetReleased(target_id.as_str().to_owned())
        } else {
            OperationError::UnknownTarget(target_id.as_str().to_owned())
        }
    }

    pub(crate) fn register_now(
        &self,
        target_id: OperationTargetId,
        handler: OperationTargetHandle,
    ) -> Result<(), OperationError> {
        {
            let mut targets = self.lock()?;
            if targets.registered.contains_key(&target_id) {
                return Err(OperationError::DuplicateTarget(
                    target_id.as_str().to_owned(),
                ));
            }
            targets.released.remove(&target_id);
            targets.registered.insert(
                target_id,
                RegisteredTarget {
                    handler,
                    gate: std::sync::Arc::default(),
                },
            );
        }
        self.publish_state();
        Ok(())
    }

    fn release_now(&self, target_id: OperationTargetId) -> Result<bool, OperationError> {
        let released = {
            let mut targets = self.lock()?;
            let released = targets.registered.remove(&target_id).is_some();
            if released {
                targets.released.insert(target_id);
            }
            released
        };
        if released {
            self.publish_state();
        }
        Ok(released)
    }

    fn begin_now(&self) -> Result<u64, OperationError> {
        let transaction_id = self.next_transaction_id();
        self.lock()?.pending.insert(transaction_id, Vec::new());
        Ok(transaction_id)
    }

    fn commit_now(&self, transaction_id: u64) -> Result<(), OperationError> {
        let pending = self
            .lock()?
            .pending
            .remove(&transaction_id)
            .ok_or(OperationError::TransactionClosed)?;
        for (receipt, _) in pending {
            self.emit(OperationEvent::Committed(receipt))?;
        }
        Ok(())
    }

    fn abort_now(&self, transaction_id: u64) -> Result<(), OperationError> {
        let pending = self
            .lock()?
            .pending
            .remove(&transaction_id)
            .ok_or(OperationError::TransactionClosed)?;
        for (receipt, handler) in pending.into_iter().rev() {
            if let Some(inverse) = receipt.inverse.as_ref() {
                handler.apply(inverse, &OperationOrigin::HistoryUndo)?;
            }
        }
        Ok(())
    }

    pub(crate) fn submit_now(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
        transaction_id: Option<u64>,
    ) -> Result<OperationReceipt, OperationError> {
        let target_id = record.target_id.clone();
        let transaction_id = transaction_id.unwrap_or_else(|| self.next_transaction_id());
        let target = {
            let targets = self.lock()?;
            match targets.registered.get(&target_id) {
                Some(target) => target.clone(),
                None => return Err(Self::missing(&targets, &target_id)),
            }
        };
        let applied = {
            let _gate = target.gate.lock().map_err(|_| OperationError::Poisoned)?;
            target.handler.apply(&record, &origin)
        };
        let result = match applied {
            Ok(result) => result,
            Err(error) => {
                self.emit_failed(transaction_id, &record, origin, &error);
                return Err(error);
            }
        };
        let receipt = OperationReceipt {
            transaction_id,
            target_id,
            forward: record,
            inverse: result.inverse,
            origin,
            status: result.status,
            affected_ids: result.affected_ids,
            before_revision: result.before_revision,
            after_revision: result.after_revision,
        };
        let queued = match self.lock()?.pending.get_mut(&transaction_id) {
            Some(pending) => {
                pending.push((receipt.clone(), target.handler));
                true
            }
            None => false,
        };
        if !queued {
            self.emit(OperationEvent::Committed(receipt.clone()))?;
        }
        Ok(receipt)
    }

    fn publish_remote_now(
        &self,
        record: OperationRecord,
        peer_id: String,
        before_revision: u64,
        after_revision: u64,
        affected_ids: Vec<String>,
    ) -> Result<OperationReceipt, OperationError> {
        if peer_id.is_empty() || peer_id.len() > 128 || after_revision <= before_revision {
            return Err(OperationError::Validation(
                "invalid_remote_publication".into(),
            ));
        }
        {
            let targets = self.lock()?;
            if !targets.registered.contains_key(&record.target_id) {
                return Err(Self::missing(&targets, &record.target_id));
            }
        }
        let receipt = OperationReceipt {
            transaction_id: self.next_transaction_id(),
            target_id: record.target_id.clone(),
            forward: record,
            inverse: None,
            origin: OperationOrigin::Remote(peer_id),
            status: OperationApplyStatus::Applied,
            affected_ids,
            before_revision,
            after_revision,
        };
        self.emit(OperationEvent::Committed(receipt.clone()))?;
        Ok(receipt)
    }

    fn emit_failed(
        &self,
        transaction_id: u64,
        record: &OperationRecord,
        origin: OperationOrigin,
        error: &OperationError,
    ) {
        let _ = self.emit(OperationEvent::Failed(OperationFailure {
            transaction_id,
            target_id: record.target_id.clone(),
            kind: record.kind.clone(),
            version: record.version,
            origin,
            error_code: error.code().to_owned(),
        }));
    }

    /// Publishes `event` (after keeping it in the diagnostics, with the feature).
    fn emit(&self, event: OperationEvent) -> Result<(), OperationError> {
        #[cfg(feature = "diagnostics")]
        {
            let mut targets = self.lock()?;
            match &event {
                OperationEvent::Committed(receipt) => {
                    targets.recent_transactions.push_back(
                        rayx_app::services::operations::OperationTransactionDiagnostics {
                            transaction_id: receipt.transaction_id,
                            target_id: receipt.target_id.clone(),
                            kind: receipt.forward.kind.clone(),
                            version: receipt.forward.version,
                            status: receipt.status,
                            origin: receipt.origin.clone(),
                            before_revision: receipt.before_revision,
                            after_revision: receipt.after_revision,
                        },
                    );
                    while targets.recent_transactions.len() > MAX_RECENT_TRANSACTIONS {
                        targets.recent_transactions.pop_front();
                    }
                }
                OperationEvent::Failed(failure) => {
                    targets
                        .recent_failure_codes
                        .push_back(failure.error_code.clone());
                    while targets.recent_failure_codes.len() > MAX_RECENT_TRANSACTIONS {
                        targets.recent_failure_codes.pop_front();
                    }
                }
            }
            drop(targets);
            self.publish_state();
        }
        self.events.publish(event);
        Ok(())
    }
}

/// A command's answer: RayX's are ready at once.
fn answer<T: Send + 'static>(value: T) -> BoxFuture<'static, T> {
    futures::future::ready(value).boxed()
}

impl OperationService for RayxOperations {
    fn operations(&self) -> &StateCell<OperationState> {
        &self.operations
    }

    fn events(&self) -> &EventStream<OperationEvent> {
        &self.events
    }

    fn instance_id(&self) -> u64 {
        self.instance_id
    }

    fn register_target(
        &self,
        target_id: OperationTargetId,
        handler: OperationTargetHandle,
    ) -> BoxFuture<'_, Result<(), OperationError>> {
        answer(self.register_now(target_id, handler))
    }

    fn release_target(
        &self,
        target_id: OperationTargetId,
    ) -> BoxFuture<'_, Result<bool, OperationError>> {
        answer(self.release_now(target_id))
    }

    fn begin(&self) -> BoxFuture<'_, Result<u64, OperationError>> {
        answer(self.begin_now())
    }

    fn commit(&self, transaction_id: u64) -> BoxFuture<'_, Result<(), OperationError>> {
        answer(self.commit_now(transaction_id))
    }

    fn abort(&self, transaction_id: u64) -> BoxFuture<'_, Result<(), OperationError>> {
        answer(self.abort_now(transaction_id))
    }

    fn submit_record(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
        transaction_id: Option<u64>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        answer(self.submit_now(record, origin, transaction_id))
    }

    fn publish_remote_record(
        &self,
        record: OperationRecord,
        peer_id: String,
        before_revision: u64,
        after_revision: u64,
        affected_ids: Vec<String>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        answer(self.publish_remote_now(
            record,
            peer_id,
            before_revision,
            after_revision,
            affected_ids,
        ))
    }
}
