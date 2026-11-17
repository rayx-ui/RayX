use std::cell::RefCell;

use futures::FutureExt as _;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use super::{OperationError, OperationService};

thread_local! {
    /// The transactions open on this thread, innermost last, by service instance.
    static TRANSACTIONS: RefCell<Vec<ActiveTransaction>> = const { RefCell::new(Vec::new()) };
}

/// How `begin_transaction` on `dyn OperationService` treats a transaction of the
/// same service open on the thread.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperationTransactionMode {
    /// Joins it (its commit and abort are the outer transaction's).
    Join,
    /// Begins a new one inside it.
    New,
    /// Fails with [`OperationError::NestedTransactionRejected`].
    Reject,
}

#[derive(Clone, Copy)]
struct ActiveTransaction {
    service_id: u64,
    transaction_id: u64,
}

/// The innermost transaction of the service `service_id` open on this thread.
pub(super) fn current_transaction_id(service_id: u64) -> Option<u64> {
    TRANSACTIONS.with(|transactions| {
        transactions
            .borrow()
            .iter()
            .rev()
            .find(|active| active.service_id == service_id)
            .map(|active| active.transaction_id)
    })
}

fn push(service_id: u64, transaction_id: u64) {
    TRANSACTIONS.with(|transactions| {
        transactions.borrow_mut().push(ActiveTransaction {
            service_id,
            transaction_id,
        });
    });
}

fn pop(service_id: u64, transaction_id: u64) {
    TRANSACTIONS.with(|transactions| {
        let mut transactions = transactions.borrow_mut();
        if let Some(index) = transactions.iter().rposition(|active| {
            active.service_id == service_id && active.transaction_id == transaction_id
        }) {
            transactions.remove(index);
        }
    });
}

/// A transaction of an operation service, open on this thread until it is committed or
/// aborted (dropped, it aborts): the operations submitted on the thread meanwhile through the
/// helpers of `dyn OperationService` (`submit`, `resubmit`) are in it, and their events are
/// published when it commits. Begun with `begin_transaction(mode)` on `dyn OperationService`,
/// over the service's commands `begin`, `commit` and `abort`.
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
                push(service_id, transaction_id);
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
        let (service, service_id, transaction_id) =
            (self.service, self.service_id, self.transaction_id);
        if !self.owns_scope {
            return futures::future::ready(Ok(())).boxed();
        }
        async move {
            let closed = if commit {
                service.commit(transaction_id).await
            } else {
                service.abort(transaction_id).await
            };
            // After the events: a subscriber hears them inside the transaction.
            pop(service_id, transaction_id);
            closed
        }
        .boxed()
    }
}

impl Drop for OperationTransaction<'_> {
    fn drop(&mut self) {
        if !self.closed && self.owns_scope {
            // RayX's service aborts at once; another's abort is started, not awaited.
            let _ = self.service.abort(self.transaction_id).now_or_never();
            pop(self.service_id, self.transaction_id);
        }
    }
}
