use super::{HistoryError, HistoryJournalV2, HistoryScopeId, HistoryService};

impl HistoryService {
    pub(super) fn ensure_scope_loaded(
        &self,
        scope_id: &HistoryScopeId,
    ) -> Result<(), HistoryError> {
        if self
            .lock_state()?
            .scopes
            .get(scope_id)
            .is_some_and(|scope| scope.loaded)
        {
            return Ok(());
        }

        let _persistence = self
            .persistence
            .lock()
            .map_err(|_| HistoryError::Poisoned)?;
        if self
            .lock_state()?
            .scopes
            .get(scope_id)
            .is_some_and(|scope| scope.loaded)
        {
            return Ok(());
        }

        let store = self
            .store
            .lock()
            .map_err(|_| HistoryError::Poisoned)?
            .clone();
        let journal = store
            .as_ref()
            .map(|store| store.load(scope_id))
            .transpose()?
            .flatten();
        let mut state = self.lock_state()?;
        let scope = state.scopes.entry(scope_id.clone()).or_default();
        if let Some(journal) = journal {
            scope.undo = journal.entries;
            scope.redo.clear();
            scope.revision = journal.revision;
            scope.persisted_revision = Some(journal.revision);
        }
        scope.loaded = true;
        Ok(())
    }

    pub(super) fn persist_scope(&self, scope_id: &HistoryScopeId) -> Result<(), HistoryError> {
        let Some(store) = self
            .store
            .lock()
            .map_err(|_| HistoryError::Poisoned)?
            .clone()
        else {
            return Ok(());
        };
        let _persistence = self
            .persistence
            .lock()
            .map_err(|_| HistoryError::Poisoned)?;
        let (journal, expected_revision) = self.persistence_snapshot(scope_id)?;
        store.save(journal.clone(), expected_revision)?;
        self.record_persisted_revision(scope_id, journal.revision)
    }

    pub(super) fn persistence_snapshot(
        &self,
        scope_id: &HistoryScopeId,
    ) -> Result<(HistoryJournalV2, Option<u64>), HistoryError> {
        let state = self.lock_state()?;
        let scope = state
            .scopes
            .get(scope_id)
            .ok_or(HistoryError::StackChanged)?;
        Ok((
            HistoryJournalV2::new(scope.revision, scope_id.clone(), scope.undo.clone()),
            scope.persisted_revision,
        ))
    }

    pub(super) fn record_persisted_revision(
        &self,
        scope_id: &HistoryScopeId,
        revision: u64,
    ) -> Result<(), HistoryError> {
        let mut state = self.lock_state()?;
        let scope = state
            .scopes
            .get_mut(scope_id)
            .ok_or(HistoryError::StackChanged)?;
        scope.persisted_revision = Some(revision);
        Ok(())
    }
}
