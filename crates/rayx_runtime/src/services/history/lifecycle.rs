use super::{HistoryError, HistoryEvent, HistoryJournalV2, HistoryScopeId, HistoryService};
use crate::services::operations::{OperationError, OperationTransaction, OperationTransactionMode};

impl HistoryService {
    /// Bounds the total retained logical entries in this scope. Unconfigured scopes remain unbounded.
    /// Reductions discard oldest undo entries first, then the most distant redo entries.
    /// Call at a session boundary, outside an operation transaction or history replay.
    pub fn set_scope_entry_limit(
        &self,
        scope_id: &HistoryScopeId,
        limit: usize,
    ) -> Result<(), HistoryError> {
        if limit == 0 {
            return Err(HistoryError::InvalidEntryLimit);
        }
        let transaction = self.begin_scope_change()?;
        self.ensure_scope_loaded(scope_id)?;
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
        let persist = evicted;
        drop(state);
        let result = if persist {
            self.persist_scope(scope_id)
        } else {
            Ok(())
        };
        transaction.commit()?;
        self.emit(HistoryEvent::AvailabilityChanged(scope_id.clone()));
        result
    }

    /// Returns undo and redo entry counts without cloning operation payloads.
    pub fn stack_depths(&self, scope_id: &HistoryScopeId) -> Result<(usize, usize), HistoryError> {
        self.ensure_scope_loaded(scope_id)?;
        Ok(self
            .lock_state()?
            .scopes
            .get(scope_id)
            .map_or((0, 0), |scope| (scope.undo.len(), scope.redo.len())))
    }

    /// Clears both stacks while retaining the explicit target binding and entry policy.
    /// Persistence failures are returned after the in-memory clear, like undo/redo save failures.
    pub fn clear_scope(&self, scope_id: &HistoryScopeId) -> Result<(), HistoryError> {
        let transaction = self.begin_scope_change()?;
        self.ensure_scope_loaded(scope_id)?;
        self.clear_scope_state(scope_id, transaction.id(), None)?;
        let result = self.persist_scope(scope_id);
        transaction.commit()?;
        self.emit(HistoryEvent::ScopeCleared(scope_id.clone()));
        result
    }

    /// [`clear_scope`](Self::clear_scope), for callers that await.
    pub async fn clear_scope_async(&self, scope_id: &HistoryScopeId) -> Result<(), HistoryError> {
        self.clear_scope(scope_id)
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

    fn begin_scope_change(&self) -> Result<OperationTransaction, HistoryError> {
        self.operations
            .begin_transaction(OperationTransactionMode::Reject)
            .map_err(|error| {
                if error == OperationError::NestedTransactionRejected {
                    HistoryError::ScopeBusy
                } else {
                    error.into()
                }
            })
    }
}
