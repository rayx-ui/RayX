//! A scope's lifecycle: its entry limit and clearing it.

use rayx_app::services::history::{HistoryError, HistoryEvent, HistoryScopeId};

use crate::journal::HistoryJournalV2;
use crate::service::{OpenTransaction, RayxHistory};

impl RayxHistory {
    /// Bounds the total retained logical entries in this scope. Unconfigured scopes remain
    /// unbounded. Reductions discard oldest undo entries first, then the most distant redo
    /// entries. Busy while the scope is replayed.
    pub(crate) async fn set_scope_entry_limit_now(
        &self,
        scope_id: &HistoryScopeId,
        limit: usize,
    ) -> Result<(), HistoryError> {
        if limit == 0 {
            return Err(HistoryError::InvalidEntryLimit);
        }
        self.ensure_scope_loaded(scope_id).await?;
        let transaction = self.begin_scope_change().await?;
        let evicted = {
            let mut state = self.lock_state()?;
            let scope = state.scopes.entry(scope_id.clone()).or_default();
            if scope.replaying {
                return Err(HistoryError::ScopeBusy);
            }
            scope.entry_limit = Some(limit);
            let evicted = scope.enforce_entry_limit();
            if evicted {
                scope.revision += 1;
            }
            evicted
        };
        self.publish_state();
        let result = if evicted {
            self.persist_scope(scope_id).await
        } else {
            Ok(())
        };
        transaction.commit().await?;
        self.emit(HistoryEvent::AvailabilityChanged(scope_id.clone()));
        result
    }

    /// Clears both stacks while retaining the explicit target binding and entry policy; busy
    /// while the scope is replayed. The operations of transactions begun before the clear (on
    /// any thread) and committed after it are not recorded. Persistence failures are returned
    /// after the in-memory clear, like undo/redo save failures.
    pub(crate) async fn clear_scope_now(
        &self,
        scope_id: &HistoryScopeId,
    ) -> Result<(), HistoryError> {
        self.ensure_scope_loaded(scope_id).await?;
        let transaction = self.begin_scope_change().await?;
        let boundary = transaction.id.unwrap_or_default();
        self.clear_scope_state(scope_id, boundary, None)?;
        self.publish_state();
        let result = self.persist_scope(scope_id).await;
        transaction.commit().await?;
        self.emit(HistoryEvent::ScopeCleared(scope_id.clone()));
        result
    }

    fn clear_scope_state(
        &self,
        scope_id: &HistoryScopeId,
        transaction_id: u64,
        loaded_journal: Option<HistoryJournalV2>,
    ) -> Result<(), HistoryError> {
        let mut state = self.lock_state()?;
        let scope = state.scopes.entry(scope_id.clone()).or_default();
        if scope.replaying {
            return Err(HistoryError::ScopeBusy);
        }
        if let Some(journal) = loaded_journal {
            scope.persisted_revision = Some(journal.revision);
            scope.revision = scope.revision.max(journal.revision);
        }
        scope.cleared_through_transaction = transaction_id;
        scope.undo.clear();
        scope.redo.clear();
        scope.revision += 1;
        scope.loaded = true;
        Ok(())
    }

    /// A transaction of its own, by id: its id is the boundary a clear discards the operations
    /// of earlier transactions by.
    async fn begin_scope_change(&self) -> Result<OpenTransaction<'_>, HistoryError> {
        Ok(OpenTransaction {
            operations: &*self.operations,
            id: Some(self.operations.begin().await?),
        })
    }
}
