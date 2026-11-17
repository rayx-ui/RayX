use std::cell::RefCell;

use futures::FutureExt as _;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use super::{DomainOperation, OperationError, OperationReceipt, OperationRecord, OperationService};
use super::{OperationOrigin, OperationTargetId};

thread_local! {
    /// The transactions entered on this thread, innermost last, by service instance: each only
    /// for the length of a synchronous [`OperationTransaction::enter`], so never across an
    /// `.await`.
    static ENTERED: RefCell<Vec<EnteredTransaction>> = const { RefCell::new(Vec::new()) };
}

/// How `begin_transaction` on `dyn OperationService` treats a transaction of the same service
/// entered on the thread (inside [`OperationTransaction::enter`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperationTransactionMode {
    /// Joins it (its commit and abort are the outer transaction's).
    Join,
    /// Begins a new one, whatever is entered.
    New,
    /// Fails with [`OperationError::NestedTransactionRejected`].
    Reject,
}

#[derive(Clone, Copy)]
struct EnteredTransaction {
    service_id: u64,
    transaction_id: u64,
}

/// The innermost transaction of the service `service_id` entered on this thread.
pub(super) fn current_transaction_id(service_id: u64) -> Option<u64> {
    ENTERED.with(|entered| {
        entered
            .borrow()
            .iter()
            .rev()
            .find(|active| active.service_id == service_id)
            .map(|active| active.transaction_id)
    })
}

/// Leaves the transaction entered last on the thread when dropped (a body that panics leaves
/// it too).
struct Entered;

impl Drop for Entered {
    fn drop(&mut self) {
        ENTERED.with(|entered| {
            entered.borrow_mut().pop();
        });
    }
}

/// A transaction of an operation service, open until it is committed or aborted (dropped, it
/// aborts). Begun with `begin_transaction(mode)` on `dyn OperationService`, over the service's
/// commands `begin`, `commit` and `abort`.
///
/// The transaction is carried explicitly: [`submit`](Self::submit) and
/// [`resubmit`](Self::resubmit) submit in it by its id, from any task or thread, so it may be
/// held across `.await`s and moved between threads, and two tasks on one thread never share
/// one. [`enter`](Self::enter) makes it the thread's ambient transaction for a synchronous body
/// only: there `submit`/`resubmit` on `dyn OperationService` (and code that calls them, like a
/// view model's handler) submit in it, and `begin_transaction(Join)` joins it. A body cannot
/// `.await`, so the ambient transaction never spans another task's work; a submission's future
/// made in the body carries the transaction's id and may be awaited after it.
pub struct OperationTransaction<'a> {
    service: &'a dyn OperationService,
    service_id: u64,
    transaction_id: u64,
    owns_scope: bool,
    closed: bool,
}

impl<'a> OperationTransaction<'a> {
    pub(super) fn begin(
        service: &'a dyn OperationService,
        mode: OperationTransactionMode,
    ) -> BoxFuture<'a, Result<Self, OperationError>> {
        let service_id = service.instance_id();
        match (mode, current_transaction_id(service_id)) {
            (OperationTransactionMode::Join, Some(transaction_id)) => {
                futures::future::ready(Ok(Self {
                    service,
                    service_id,
                    transaction_id,
                    owns_scope: false,
                    closed: false,
                }))
                .boxed()
            }
            (OperationTransactionMode::Reject, Some(_)) => {
                futures::future::ready(Err(OperationError::NestedTransactionRejected)).boxed()
            }
            _ => async move {
                let transaction_id = service.begin().await?;
                Ok(Self {
                    service,
                    service_id,
                    transaction_id,
                    owns_scope: true,
                    closed: false,
                })
            }
            .boxed(),
        }
    }

    /// The transaction's id.
    pub fn id(&self) -> u64 {
        self.transaction_id
    }

    /// Whether it joined an outer transaction.
    pub fn is_joined(&self) -> bool {
        !self.owns_scope
    }

    /// Submits `operation` to `target_id` from `origin` in this transaction.
    pub fn submit<T: DomainOperation>(
        &self,
        target_id: &OperationTargetId,
        operation: T,
        origin: OperationOrigin,
    ) -> BoxFuture<'a, Result<OperationReceipt, OperationError>> {
        match OperationRecord::from_operation(target_id.clone(), &operation) {
            Ok(record) => self.resubmit(record, origin),
            Err(error) => futures::future::ready(Err(error)).boxed(),
        }
    }

    /// Submits `record` from `origin` in this transaction.
    pub fn resubmit(
        &self,
        record: OperationRecord,
        origin: OperationOrigin,
    ) -> BoxFuture<'a, Result<OperationReceipt, OperationError>> {
        self.service
            .submit_record(record, origin, Some(self.transaction_id))
    }

    /// Runs `body` with this transaction entered on the thread: the submissions made in it
    /// through `submit`/`resubmit` on `dyn OperationService` are in this transaction, and
    /// `begin_transaction(Join)` joins it. It is left when `body` returns (or panics). `body`
    /// is synchronous, so it never spans an `.await`; the futures it makes carry the id.
    pub fn enter<R>(&self, body: impl FnOnce() -> R) -> R {
        ENTERED.with(|entered| {
            entered.borrow_mut().push(EnteredTransaction {
                service_id: self.service_id,
                transaction_id: self.transaction_id,
            });
        });
        let _entered = Entered;
        body()
    }

    /// Commits it: its operations' events are published (a joined transaction leaves that to
    /// the outer one).
    pub fn commit(self) -> BoxFuture<'a, Result<(), OperationError>> {
        self.close(true)
    }

    /// Aborts it: its operations are undone with their inverses, newest first, and publish
    /// nothing (a joined transaction leaves that to the outer one).
    pub fn abort(self) -> BoxFuture<'a, Result<(), OperationError>> {
        self.close(false)
    }

    fn close(mut self, commit: bool) -> BoxFuture<'a, Result<(), OperationError>> {
        self.closed = true;
        if !self.owns_scope {
            return futures::future::ready(Ok(())).boxed();
        }
        if commit {
            self.service.commit(self.transaction_id)
        } else {
            self.service.abort(self.transaction_id)
        }
    }
}

impl Drop for OperationTransaction<'_> {
    fn drop(&mut self) {
        if !self.closed && self.owns_scope {
            // RayX's service aborts at once; another's abort is started, not awaited.
            let _ = self.service.abort(self.transaction_id).now_or_never();
        }
    }
}
