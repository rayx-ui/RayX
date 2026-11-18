//! Loading a scope from the history's store and saving it there, asynchronously: a scope is
//! loaded the first time a command uses it (an operation recorded before its load is kept above
//! the loaded entries), and saved behind each change by one writer at a time, which runs
//! detached: a command awaits only the outcome of its save, so a command given up on leaves no
//! save half done. A save that meets another revision in the store than it expects reloads it
//! and saves over it.

use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::BoxFuture;
use rayx_app::services::history::{HistoryError, HistoryScopeId};

use crate::journal::HistoryJournalV2;
use crate::service::RayxHistory;
use crate::store::{HistoryStore, HistoryStoreError};

/// How many times a save that meets another revision reloads it and saves again.
const SAVE_ATTEMPTS: usize = 3;

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
    /// of one scope may run at once: the first to finish is kept. Before the app's store is
    /// attached (its startup), the scope stays unloaded: the writer loads it then.
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
            None if self.store_configured => return Ok(()),
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

    /// Saves the scope behind its change; the outcome of the save that holds it. One writer,
    /// detached, saves at a time, each scope at its latest revision; dropping the returned
    /// future gives up only the wait. Before the app's store is attached, the scope is left
    /// unsaved, saved once it is, and this answers at once.
    pub(crate) fn persist_scope(
        &self,
        scope_id: &HistoryScopeId,
    ) -> BoxFuture<'static, Result<(), HistoryError>> {
        if !self.store_configured {
            return futures::future::ready(Ok(())).boxed();
        }
        let attached = match self.current_store() {
            Ok(store) => store.is_some(),
            Err(error) => return futures::future::ready(Err(error)).boxed(),
        };
        let (answer, answered) = futures::channel::oneshot::channel();
        let start = {
            let mut state = match self.lock_state() {
                Ok(state) => state,
                Err(error) => return futures::future::ready(Err(error)).boxed(),
            };
            state.unsaved.insert(scope_id.clone());
            if attached {
                state
                    .save_waiters
                    .entry(scope_id.clone())
                    .or_default()
                    .push(answer);
            }
            attached && !std::mem::replace(&mut state.writing, true)
        };
        if start {
            self.start_writer();
        }
        if !attached {
            return futures::future::ready(Ok(())).boxed();
        }
        // A writer that ended without answering (the history went away) saved nothing more.
        async move { answered.await.unwrap_or(Ok(())) }.boxed()
    }

    /// Saves `scope_id` where nothing awaits it: at once with a store that answers at once,
    /// else when the store answers; a failure is a failed step of the scope.
    pub(crate) fn persist_behind(&self, scope_id: &HistoryScopeId) {
        // Nobody waits: the save's outcome reaches the scope's failures.
        drop(self.persist_scope(scope_id));
    }

    /// Starts the writer for what was changed before the app's store was attached.
    pub(crate) fn save_unsaved(&self) {
        let start = match self.lock_state() {
            Ok(mut state) => {
                !state.unsaved.is_empty() && !std::mem::replace(&mut state.writing, true)
            }
            Err(_) => false,
        };
        if start {
            self.start_writer();
        }
    }

    /// Runs the writer detached (its turn taken already).
    fn start_writer(&self) {
        let Some(history) = self.this.upgrade() else {
            if let Ok(mut state) = self.lock_state() {
                state.writing = false;
            }
            return;
        };
        crate::detached::spawn(Box::pin(async move { history.write().await }));
    }

    /// Saves the unsaved scopes until none is left, answering each one's waiters.
    async fn write(self: Arc<Self>) {
        let Ok(Some(store)) = self.current_store() else {
            if let Ok(mut state) = self.lock_state() {
                state.writing = false;
            }
            return;
        };
        loop {
            let (next, waiters) = {
                let Ok(mut state) = self.lock_state() else {
                    return;
                };
                match state.unsaved.pop_first() {
                    Some(next) => {
                        let waiters = state.save_waiters.remove(&next).unwrap_or_default();
                        (next, waiters)
                    }
                    None => {
                        // Left under the lock that found nothing left, so a change made
                        // meanwhile finds no writer and starts one.
                        state.writing = false;
                        return;
                    }
                }
            };
            let saved = self.save_scope(&*store, &next).await;
            if waiters.is_empty() {
                if let Err(error) = &saved {
                    self.record_failure(next, error);
                }
            } else {
                for waiter in waiters {
                    let _ = waiter.send(saved.clone());
                }
            }
        }
    }

    /// Saves `scope_id` at its revision now (loading it first if it was not); a save that
    /// meets another revision in the store reloads it, and saves over it unless it holds this
    /// very journal.
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
        let mut attempt = 1;
        loop {
            let (journal, expected_revision) = self.persistence_snapshot(scope_id)?;
            if expected_revision == Some(journal.revision) {
                return Ok(());
            }
            let revision = journal.revision;
            match store.save(journal.clone(), expected_revision).await {
                Ok(()) => return self.record_persisted_revision(scope_id, revision),
                Err(HistoryStoreError::Conflict) if attempt < SAVE_ATTEMPTS => {
                    attempt += 1;
                    let stored = store.load(scope_id).await?;
                    if stored.as_ref() == Some(&journal) {
                        // This journal, written by a save whose outcome was lost.
                        return self.record_persisted_revision(scope_id, revision);
                    }
                    self.take_stored_revision(scope_id, stored.map(|stored| stored.revision))?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Expects `stored` in the store from now on, the scope's revision past it.
    fn take_stored_revision(
        &self,
        scope_id: &HistoryScopeId,
        stored: Option<u64>,
    ) -> Result<(), HistoryError> {
        {
            let mut state = self.lock_state()?;
            let scope = state
                .scopes
                .get_mut(scope_id)
                .ok_or(HistoryError::StackChanged)?;
            scope.persisted_revision = stored;
            if let Some(stored) = stored
                && stored >= scope.revision
            {
                scope.revision = stored + 1;
            }
        }
        self.publish_state();
        Ok(())
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
        // A later save may have recorded a later revision already.
        if scope
            .persisted_revision
            .is_none_or(|persisted| persisted < revision)
        {
            scope.persisted_revision = Some(revision);
        }
        Ok(())
    }
}
