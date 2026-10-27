use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use super::{
    DomainOperation, ErasedOperationTarget, OperationError, OperationEvent, OperationFailure,
    OperationOrigin, OperationReceipt, OperationRecord, OperationTargetHandler, OperationTargetId,
    OperationTransaction, OperationTransactionMode, TypedOperationTarget, current_transaction_id,
};
use futures::{FutureExt, future::LocalBoxFuture};
use rayx_core::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};

#[cfg(feature = "diagnostics")]
const MAX_RECENT_TRANSACTIONS: usize = 32;

type OperationSubscriber = Arc<dyn Fn(&OperationEvent) + Send + Sync>;

#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationTransactionDiagnostics {
    pub transaction_id: u64,
    pub target_id: OperationTargetId,
    pub kind: String,
    pub version: u32,
    pub status: super::OperationApplyStatus,
    pub origin: super::OperationOrigin,
    pub before_revision: u64,
    pub after_revision: u64,
}

#[cfg(feature = "diagnostics")]
impl OperationTransactionDiagnostics {
    pub fn contains_payload_values(&self) -> bool {
        false
    }
}

#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDiagnostics {
    pub registered_target_count: usize,
    pub subscriber_count: usize,
    pub recent_transactions: Vec<OperationTransactionDiagnostics>,
    pub recent_failure_codes: Vec<String>,
}

#[derive(Default)]
struct OperationState {
    targets: BTreeMap<OperationTargetId, Arc<RegisteredTarget>>,
    released_targets: BTreeSet<OperationTargetId>,
    pending_transactions: BTreeMap<u64, Vec<PendingReceipt>>,
    subscribers: BTreeMap<u64, OperationSubscriber>,
    next_subscriber_id: u64,
    #[cfg(feature = "diagnostics")]
    recent_transactions: std::collections::VecDeque<OperationTransactionDiagnostics>,
    #[cfg(feature = "diagnostics")]
    recent_failure_codes: std::collections::VecDeque<String>,
}

struct RegisteredTarget {
    handler: Arc<dyn ErasedOperationTarget>,
    gate: Mutex<()>,
}

struct PendingReceipt {
    receipt: OperationReceipt,
    target: Arc<dyn ErasedOperationTarget>,
}

#[derive(Clone)]
pub struct OperationService {
    instance_id: u64,
    next_transaction: Arc<AtomicU64>,
    state: Arc<Mutex<OperationState>>,
    initialization: StartupInitializationState,
}

pub struct OperationSubscription {
    state: Weak<Mutex<OperationState>>,
    id: u64,
}

impl Drop for OperationSubscription {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.subscribers.remove(&self.id);
        }
    }
}

impl Default for OperationService {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationService {
    pub fn new() -> Self {
        static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(0);
        Self {
            instance_id: NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed) + 1,
            next_transaction: Arc::new(AtomicU64::new(0)),
            state: Arc::new(Mutex::new(OperationState::default())),
            initialization: StartupInitializationState::default(),
        }
    }

    pub fn register_target<T, H>(
        &self,
        target_id: OperationTargetId,
        handler: Arc<H>,
    ) -> Result<(), OperationError>
    where
        T: DomainOperation,
        H: OperationTargetHandler<T>,
    {
        let mut state = self.lock_state()?;
        if state.targets.contains_key(&target_id) {
            return Err(OperationError::DuplicateTarget(
                target_id.as_str().to_string(),
            ));
        }
        state.released_targets.remove(&target_id);
        state.targets.insert(
            target_id.clone(),
            Arc::new(RegisteredTarget {
                handler: Arc::new(TypedOperationTarget::<T, H>::new(target_id, handler)),
                gate: Mutex::new(()),
            }),
        );
        Ok(())
    }

    pub fn release_target(&self, target_id: &OperationTargetId) -> Result<bool, OperationError> {
        let mut state = self.lock_state()?;
        let released = state.targets.remove(target_id).is_some();
        if released {
            state.released_targets.insert(target_id.clone());
        }
        Ok(released)
    }

    pub fn begin_transaction(
        &self,
        mode: OperationTransactionMode,
    ) -> Result<OperationTransaction, OperationError> {
        OperationTransaction::begin(self.clone(), mode)
    }

    pub fn submit<T: DomainOperation>(
        &self,
        target_id: &OperationTargetId,
        operation: T,
        origin: OperationOrigin,
    ) -> Result<OperationReceipt, OperationError> {
        let record = OperationRecord::from_operation(target_id.clone(), &operation)?;
        self.submit_record(record, origin)
    }

    pub fn resubmit(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
    ) -> Result<OperationReceipt, OperationError> {
        self.submit_record(record, origin)
    }

    pub fn publish_remote<T: DomainOperation>(
        &self,
        target_id: &OperationTargetId,
        operation: T,
        peer_id: impl Into<String>,
        before_revision: u64,
        after_revision: u64,
        affected_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<OperationReceipt, OperationError> {
        let peer_id = peer_id.into();
        if peer_id.is_empty() || peer_id.len() > 128 || after_revision <= before_revision {
            return Err(OperationError::Validation(
                "invalid_remote_publication".into(),
            ));
        }
        {
            let state = self.lock_state()?;
            if !state.targets.contains_key(target_id) {
                return Err(if state.released_targets.contains(target_id) {
                    OperationError::TargetReleased(target_id.as_str().to_string())
                } else {
                    OperationError::UnknownTarget(target_id.as_str().to_string())
                });
            }
        }
        let receipt = OperationReceipt {
            transaction_id: self.next_transaction_id(),
            target_id: target_id.clone(),
            forward: OperationRecord::from_operation(target_id.clone(), &operation)?,
            inverse: None,
            origin: OperationOrigin::Remote(peer_id),
            status: super::OperationApplyStatus::Applied,
            affected_ids: affected_ids.into_iter().map(Into::into).collect(),
            before_revision,
            after_revision,
        };
        self.emit(OperationEvent::Committed(receipt.clone()))?;
        Ok(receipt)
    }

    pub(crate) fn submit_record(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
    ) -> Result<OperationReceipt, OperationError> {
        let target_id = record.target_id.clone();
        let transaction_id =
            current_transaction_id(self.instance_id).unwrap_or_else(|| self.next_transaction_id());
        let target = {
            let state = self.lock_state()?;
            match state.targets.get(&target_id).cloned() {
                Some(target) => target,
                None if state.released_targets.contains(&target_id) => {
                    return Err(OperationError::TargetReleased(
                        target_id.as_str().to_string(),
                    ));
                }
                None => {
                    return Err(OperationError::UnknownTarget(
                        target_id.as_str().to_string(),
                    ));
                }
            }
        };
        let apply_result = {
            let _gate = target.gate.lock().map_err(|_| OperationError::Poisoned)?;
            target.handler.apply(&record, &origin)
        };
        let result = match apply_result {
            Ok(result) => result,
            Err(error) => {
                self.emit_failed(transaction_id, &target_id, &record, origin, &error);
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
        let queued = {
            let mut state = self.lock_state()?;
            if let Some(pending) = state.pending_transactions.get_mut(&transaction_id) {
                pending.push(PendingReceipt {
                    receipt: receipt.clone(),
                    target: target.handler.clone(),
                });
                true
            } else {
                false
            }
        };
        if !queued {
            self.emit(OperationEvent::Committed(receipt.clone()))?;
        }
        Ok(receipt)
    }

    pub fn subscribe(
        &self,
        subscriber: impl Fn(&OperationEvent) + Send + Sync + 'static,
    ) -> Result<OperationSubscription, OperationError> {
        let mut state = self.lock_state()?;
        state.next_subscriber_id += 1;
        let id = state.next_subscriber_id;
        state.subscribers.insert(id, Arc::new(subscriber));
        Ok(OperationSubscription {
            state: Arc::downgrade(&self.state),
            id,
        })
    }

    #[cfg(feature = "diagnostics")]
    pub fn diagnostics(&self) -> OperationDiagnostics {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        OperationDiagnostics {
            registered_target_count: state.targets.len(),
            subscriber_count: state.subscribers.len(),
            recent_transactions: state.recent_transactions.iter().cloned().collect(),
            recent_failure_codes: state.recent_failure_codes.iter().cloned().collect(),
        }
    }

    pub(crate) fn instance_id(&self) -> u64 {
        self.instance_id
    }

    pub(crate) fn next_transaction_id(&self) -> u64 {
        self.next_transaction.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn open_transaction(&self, transaction_id: u64) -> Result<(), OperationError> {
        let mut state = self.lock_state()?;
        state
            .pending_transactions
            .insert(transaction_id, Vec::new());
        Ok(())
    }

    pub(crate) fn commit_transaction(&self, transaction_id: u64) -> Result<(), OperationError> {
        let pending = self
            .lock_state()?
            .pending_transactions
            .remove(&transaction_id)
            .ok_or(OperationError::TransactionClosed)?;
        for pending in pending {
            self.emit(OperationEvent::Committed(pending.receipt))?;
        }
        Ok(())
    }

    pub(crate) fn abort_transaction(&self, transaction_id: u64) -> Result<(), OperationError> {
        let pending = self
            .lock_state()?
            .pending_transactions
            .remove(&transaction_id)
            .ok_or(OperationError::TransactionClosed)?;
        for pending in pending.into_iter().rev() {
            let Some(inverse) = pending.receipt.inverse.as_ref() else {
                continue;
            };
            pending
                .target
                .apply(inverse, &OperationOrigin::HistoryUndo)?;
        }
        Ok(())
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, OperationState>, OperationError> {
        self.state.lock().map_err(|_| OperationError::Poisoned)
    }

    fn emit_failed(
        &self,
        transaction_id: u64,
        target_id: &OperationTargetId,
        record: &OperationRecord,
        origin: OperationOrigin,
        error: &OperationError,
    ) {
        let _ = self.emit(OperationEvent::Failed(OperationFailure {
            transaction_id,
            target_id: target_id.clone(),
            kind: record.kind.clone(),
            version: record.version,
            origin,
            error_code: error.code().to_string(),
        }));
    }

    fn emit(&self, event: OperationEvent) -> Result<(), OperationError> {
        let subscribers = {
            #[cfg(feature = "diagnostics")]
            let mut state = self.lock_state()?;
            #[cfg(not(feature = "diagnostics"))]
            let state = self.lock_state()?;
            #[cfg(feature = "diagnostics")]
            match &event {
                OperationEvent::Committed(receipt) => {
                    state
                        .recent_transactions
                        .push_back(OperationTransactionDiagnostics {
                            transaction_id: receipt.transaction_id,
                            target_id: receipt.target_id.clone(),
                            kind: receipt.forward.kind.clone(),
                            version: receipt.forward.version,
                            status: receipt.status,
                            origin: receipt.origin.clone(),
                            before_revision: receipt.before_revision,
                            after_revision: receipt.after_revision,
                        });
                    while state.recent_transactions.len() > MAX_RECENT_TRANSACTIONS {
                        state.recent_transactions.pop_front();
                    }
                }
                OperationEvent::Failed(failure) => {
                    state
                        .recent_failure_codes
                        .push_back(failure.error_code.clone());
                    while state.recent_failure_codes.len() > MAX_RECENT_TRANSACTIONS {
                        state.recent_failure_codes.pop_front();
                    }
                }
            }
            state.subscribers.values().cloned().collect::<Vec<_>>()
        };
        for subscriber in subscribers {
            let _ = catch_unwind(AssertUnwindSafe(|| subscriber(&event)));
        }
        Ok(())
    }
}

impl AsyncInitializable for OperationService {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialization = self.initialization.clone();
        async move {
            initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for OperationService {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }
}
