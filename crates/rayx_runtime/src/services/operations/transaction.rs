use std::cell::RefCell;

use super::{OperationError, OperationService};

thread_local! {
    static TRANSACTIONS: RefCell<Vec<ActiveTransaction>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationTransactionMode {
    Join,
    New,
    Reject,
}

#[derive(Clone, Copy)]
struct ActiveTransaction {
    service_id: u64,
    transaction_id: u64,
}

pub struct OperationTransaction {
    service: OperationService,
    transaction_id: u64,
    owns_scope: bool,
    closed: bool,
}

impl OperationTransaction {
    pub(crate) fn begin(
        service: OperationService,
        mode: OperationTransactionMode,
    ) -> Result<Self, OperationError> {
        let existing = TRANSACTIONS.with(|transactions| {
            transactions
                .borrow()
                .iter()
                .rev()
                .find(|active| active.service_id == service.instance_id())
                .copied()
        });
        match (mode, existing) {
            (OperationTransactionMode::Join, Some(existing)) => Ok(Self {
                service,
                transaction_id: existing.transaction_id,
                owns_scope: false,
                closed: false,
            }),
            (OperationTransactionMode::Reject, Some(_)) => {
                Err(OperationError::NestedTransactionRejected)
            }
            (_, _) => {
                let transaction_id = service.next_transaction_id();
                service.open_transaction(transaction_id)?;
                TRANSACTIONS.with(|transactions| {
                    transactions.borrow_mut().push(ActiveTransaction {
                        service_id: service.instance_id(),
                        transaction_id,
                    });
                });
                Ok(Self {
                    service,
                    transaction_id,
                    owns_scope: true,
                    closed: false,
                })
            }
        }
    }

    pub fn id(&self) -> u64 {
        self.transaction_id
    }

    pub fn is_joined(&self) -> bool {
        !self.owns_scope
    }

    pub fn commit(mut self) -> Result<(), OperationError> {
        self.close(true)
    }

    pub fn abort(mut self) -> Result<(), OperationError> {
        self.close(false)
    }

    fn close(&mut self, commit: bool) -> Result<(), OperationError> {
        if self.closed {
            return Err(OperationError::TransactionClosed);
        }
        self.closed = true;
        if self.owns_scope {
            if commit {
                self.service.commit_transaction(self.transaction_id)?;
            } else {
                self.service.abort_transaction(self.transaction_id)?;
            }
            TRANSACTIONS.with(|transactions| {
                let mut transactions = transactions.borrow_mut();
                let Some(index) = transactions.iter().rposition(|active| {
                    active.service_id == self.service.instance_id()
                        && active.transaction_id == self.transaction_id
                }) else {
                    return Err(OperationError::TransactionClosed);
                };
                transactions.remove(index);
                Ok(())
            })?;
        }
        Ok(())
    }
}

impl Drop for OperationTransaction {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close(false);
        }
    }
}

pub(crate) fn current_transaction_id(service_id: u64) -> Option<u64> {
    TRANSACTIONS.with(|transactions| {
        transactions
            .borrow()
            .iter()
            .rev()
            .find(|active| active.service_id == service_id)
            .map(|active| active.transaction_id)
    })
}
