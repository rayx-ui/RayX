//! Loading a scope from the history's store and saving it there, asynchronously: a scope is
//! loaded the first time a command uses it (an operation recorded before its load is kept above
//! the loaded entries), and saved behind each change by one writer at a time.

use std::sync::Arc;

use rayx_app::services::history::{HistoryError, HistoryScopeId};

use crate::journal::HistoryJournalV2;
use crate::service::RayxHistory;
use crate::store::HistoryStore;

impl RayxHistory {
    /// The history's store, if it has one.
    fn current_store(&self) -> Result<Option<Arc<dyn HistoryStore>>, HistoryError> {
        Ok(self
            .store
            .lock()
            .map_err(|_| HistoryError::Poisoned)?
            .clone())
    }

    /// Loads the scope from the store the first time it is used (and publishes it). Two loads
    /// of one scope may run at once: the first to finish is kept.
    pub(crate) async fn ensure_scope_loaded(
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
        let journal = match self.current_store()? {
            Some(store) => store.load(scope_id).await?,
            None => None,
        };
        self.merge_loaded(scope_id, journal)?;
        self.publish_state();
        Ok(())
    }

    /// Puts the loaded `journal` under what the scope recorded since (unless it was loaded
    /// meanwhile).
    fn merge_loaded(
        &self,
        scope_id: &HistoryScopeId,
        journal: Option<HistoryJournalV2>,
    ) -> Result<(), HistoryError> {
        let mut state = self.lock_state()?;
        let scope = state.scopes.entry(scope_id.clone()).or_default();
        if scope.loaded {
            return Ok(());
        }
        if let Some(journal) = journal {
            let recorded = std::mem::replace(&mut scope.undo, journal.entries);
            let had_recorded = !recorded.is_empty();
            scope.undo.extend(recorded);
            // The changes made before the load count on from the saved revision.
            scope.revision += journal.revision;
            scope.persisted_revision = Some(journal.revision);
            if had_recorded {
                scope.enforce_entry_limit();
            }
        }
        scope.loaded = true;
        Ok(())
    }

    /// Saves the scope behind its change. One writer saves at a time, each scope at its latest
    /// revision: when another is saving, the scope is left to it and this answers at once
    /// (its failure, if any, is reported as a failed step of the scope). Otherwise this saves
    /// until nothing is left and answers the failure of `scope_id`'s save.
    pub(crate) async fn persist_scope(
        &self,
        scope_id: &HistoryScopeId,
    ) -> Result<(), HistoryError> {
        let Some(store) = self.current_store()? else {
            return Ok(());
        };
        {
            let mut state = self.lock_state()?;
            state.unsaved.insert(scope_id.clone());
            if state.writing {
                return Ok(());
            }
            state.writing = true;
        }
        // Dropped mid-save (its caller gave up), the writer lets the next one write.
        let mut writer = Writer(Some(self));
        let mut answer = Ok(());
        loop {
            let next = {
                let mut state = self.lock_state()?;
                match state.unsaved.pop_first() {
                    Some(next) => next,
                    None => {
                        // Left under the lock that found nothing left, so a change made
                        // meanwhile finds no writer and writes itself.
                        state.writing = false;
                        writer.0 = None;
                        break;
                    }
                }
            };
            let saved = self.save_scope(&*store, &next).await;
            if next == *scope_id {
                answer = saved;
            } else if let Err(error) = saved {
                self.record_failure(next, &error);
            }
        }
        answer
    }

    /// Saves `scope_id` at its revision now (loading it first if it was not).
    async fn save_scope(
        &self,
        store: &dyn HistoryStore,
        scope_id: &HistoryScopeId,
    ) -> Result<(), HistoryError> {
        let unloaded = self
            .lock_state()?
            .scopes
            .get(scope_id)
            .is_some_and(|scope| !scope.loaded);
        if unloaded {
            let journal = store.load(scope_id).await?;
            self.merge_loaded(scope_id, journal)?;
            self.publish_state();
        }
        let (journal, expected_revision) = self.persistence_snapshot(scope_id)?;
        if expected_revision == Some(journal.revision) {
            return Ok(());
        }
        let revision = journal.revision;
        store.save(journal, expected_revision).await?;
        self.record_persisted_revision(scope_id, revision)
    }

    fn persistence_snapshot(
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

    fn record_persisted_revision(
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

/// The writer's turn: while it lives, other changes are left to it; dropped before it finished,
/// it ends the turn.
struct Writer<'a>(Option<&'a RayxHistory>);

impl Drop for Writer<'_> {
    fn drop(&mut self) {
        if let Some(history) = self.0 {
            history
                .scopes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .writing = false;
        }
    }
}
