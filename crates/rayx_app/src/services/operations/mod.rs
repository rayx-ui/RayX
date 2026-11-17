//! Operations: typed, serializable changes of an app's targets (a document, a property grid),
//! each applied atomically by its target's handler and answered with its inverse, grouped in
//! transactions, published as [`OperationEvent`]s by an [`OperationService`] implementation:
//! what history undoes and redoes, and what a collaboration layer sends to peers.
//!
//! RayX's implementation is `rayx_services_operations`' (`add_operations()`); an app may register
//! its own, in Rust or in TS. A target's handler crosses as an [`OperationTargetHandle`], a Rust
//! object a TS implementation calls through its class handle (`OperationTarget` in TS). Typed
//! submissions (`submit::<T>`), transactions (`begin_transaction(mode)`, carried explicitly and
//! entered on the thread only for a synchronous body) and typed handlers
//! (`register_handler::<T, H>`) are helpers on `dyn OperationService`, over the contract's
//! members.

mod error;
mod model;
mod record;
mod target;
mod transaction;

pub use error::OperationError;
pub use model::{
    OperationApplyResult, OperationApplyStatus, OperationDiagnostics, OperationEvent,
    OperationFailure, OperationOrigin, OperationReceipt, OperationState,
    OperationTransactionDiagnostics,
};
pub use record::{
    DomainOperation, MAX_OPERATION_PAYLOAD_BYTES, OperationRecord, OperationTargetId,
};
pub use target::{
    OperationTarget, OperationTargetHandle, OperationTargetHandler, OperationTargetResult,
};
pub use transaction::{OperationTransaction, OperationTransactionMode};

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::BoxFuture;

use crate::ChangeSubscription;
use crate::published::{EventStream, StateCell};

/// The operation service: targets registered by id with their handlers, operations submitted to
/// them (applied at once, their events published at once or when their transaction commits),
/// transactions by id and remote operations published as committed. Its state is the
/// registered targets (and an implementation's diagnostics), its events every committed or
/// failed operation.
#[rayx_app_macros::contract(ts = "OperationServiceContract")]
pub trait OperationService {
    /// The registered targets (and diagnostics), now and as they change.
    #[state(ts = "OperationState")]
    fn operations(&self) -> &StateCell<OperationState>;

    /// Every operation committed (outside a transaction at once, inside one when it commits)
    /// or failed.
    #[events(ts = "OperationEvent")]
    fn events(&self) -> &EventStream<OperationEvent>;

    /// The service's instance: what tells two services apart (the transactions entered on a
    /// thread are kept per instance).
    #[constant]
    fn instance_id(&self) -> u64;

    /// Registers `handler` as the target `target_id` (refused when one is registered; a
    /// released id may be registered again).
    #[ts("(targetId: string, handler: OperationTargetHandle)")]
    async fn register_target(
        &self,
        target_id: OperationTargetId,
        handler: OperationTargetHandle,
    ) -> Result<(), OperationError>;

    /// Releases the target `target_id`: operations submitted to it fail with
    /// [`OperationError::TargetReleased`]. Whether it was registered.
    #[ts("(targetId: string): boolean")]
    async fn release_target(&self, target_id: OperationTargetId) -> Result<bool, OperationError>;

    /// Opens a transaction: the operations submitted in it are applied at once and published
    /// when it commits. Its id.
    async fn begin(&self) -> Result<u64, OperationError>;

    /// Commits the transaction `transaction_id`: its operations' events are published, in
    /// order.
    async fn commit(&self, transaction_id: u64) -> Result<(), OperationError>;

    /// Aborts the transaction `transaction_id`: its operations are undone with their inverses,
    /// newest first, and publish nothing.
    async fn abort(&self, transaction_id: u64) -> Result<(), OperationError>;

    /// Applies `record` from `origin` through its target: alone (published at once) with no
    /// `transaction_id`, else in that transaction, which must be open
    /// ([`OperationError::TransactionClosed`] otherwise, and the record is not applied, or is
    /// undone when the transaction closed while it applied). A failure publishes
    /// [`OperationEvent::Failed`].
    #[ts(
        "(record: OperationRecord, origin: OperationOrigin, transactionId: number | null): OperationReceipt"
    )]
    async fn submit_record(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
        transaction_id: Option<u64>,
    ) -> Result<OperationReceipt, OperationError>;

    /// Publishes `record`, which the peer `peer_id` applied (its target moved from
    /// `before_revision` to `after_revision`), as committed; the target must be registered.
    #[ts(
        "(record: OperationRecord, peerId: string, beforeRevision: number, afterRevision: number, affectedIds: string[]): OperationReceipt"
    )]
    async fn publish_remote_record(
        &self,
        record: OperationRecord,
        peer_id: String,
        before_revision: u64,
        after_revision: u64,
        affected_ids: Vec<String>,
    ) -> Result<OperationReceipt, OperationError>;
}

impl dyn OperationService {
    /// Registers `handler`, a handler of `T`'s operations, as the target `target_id`.
    pub fn register_handler<T, H>(
        &self,
        target_id: OperationTargetId,
        handler: Arc<H>,
    ) -> BoxFuture<'_, Result<(), OperationError>>
    where
        T: DomainOperation,
        H: OperationTargetHandler<T>,
    {
        let handle = OperationTargetHandle::of::<T, H>(target_id.clone(), handler);
        self.register_target(target_id, handle)
    }

    /// Begins a transaction (`mode` says what to do when one is entered on this thread): submit
    /// in it with its own `submit`/`resubmit` (from any task or thread), or enter it for a
    /// synchronous body ([`OperationTransaction::enter`]), where [`submit`](Self::submit) and
    /// [`resubmit`](Self::resubmit) submit in it.
    pub fn begin_transaction(
        &self,
        mode: OperationTransactionMode,
    ) -> BoxFuture<'_, Result<OperationTransaction<'_>, OperationError>> {
        OperationTransaction::begin(self, mode)
    }

    /// The innermost transaction entered on this thread (inside
    /// [`OperationTransaction::enter`]), if there is one.
    pub fn current_transaction_id(&self) -> Option<u64> {
        transaction::current_transaction_id(self.instance_id())
    }

    /// Submits `operation` to `target_id` from `origin`, in the transaction entered on this
    /// thread if there is one (read now: the future carries its id).
    pub fn submit<T: DomainOperation>(
        &self,
        target_id: &OperationTargetId,
        operation: T,
        origin: OperationOrigin,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        match OperationRecord::from_operation(target_id.clone(), &operation) {
            Ok(record) => self.resubmit(record, origin),
            Err(error) => futures::future::ready(Err(error)).boxed(),
        }
    }

    /// Submits `record` (an imported record) from `origin`, in the transaction entered on this
    /// thread if there is one (read now: the future carries its id).
    pub fn resubmit(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        self.submit_record(record, origin, self.current_transaction_id())
    }

    /// Publishes `operation`, which the peer `peer_id` applied to `target_id`, as committed.
    pub fn publish_remote<T: DomainOperation>(
        &self,
        target_id: &OperationTargetId,
        operation: T,
        peer_id: impl Into<String>,
        before_revision: u64,
        after_revision: u64,
        affected_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> BoxFuture<'_, Result<OperationReceipt, OperationError>> {
        match OperationRecord::from_operation(target_id.clone(), &operation) {
            Ok(record) => self.publish_remote_record(
                record,
                peer_id.into(),
                before_revision,
                after_revision,
                affected_ids.into_iter().map(Into::into).collect(),
            ),
            Err(error) => futures::future::ready(Err(error)).boxed(),
        }
    }

    /// Calls `subscriber` with every operation event, until the subscription is dropped; a
    /// subscriber that panics neither stops the others nor fails the operation.
    pub fn subscribe(
        &self,
        subscriber: impl Fn(&OperationEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            let _ = catch_unwind(AssertUnwindSafe(|| subscriber(event)));
        })
    }

    /// The registered targets, sorted.
    pub fn targets(&self) -> Vec<OperationTargetId> {
        self.operations().get().targets.clone()
    }

    /// Whether the target `target_id` is registered.
    pub fn has_target(&self, target_id: &OperationTargetId) -> bool {
        self.operations().get().has_target(target_id)
    }

    /// The service's targets, subscribers and latest operations.
    pub fn diagnostics(&self) -> OperationDiagnostics {
        let state = self.operations().get();
        OperationDiagnostics {
            registered_target_count: state.targets.len(),
            subscriber_count: self.events().subscribers(),
            recent_transactions: state.recent_transactions.clone(),
            recent_failure_codes: state.recent_failure_codes.clone(),
        }
    }
}
