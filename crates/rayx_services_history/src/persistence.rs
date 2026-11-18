//! Loading a scope from the history's store and saving it there, asynchronously: a scope is
//! loaded the first time a command uses it, and saved behind each change by one writer at a
//! time, which runs detached: a command awaits only the outcome of its save, so a command given
//! up on leaves no save half done. What a scope changed since it last loaded or saved is rebased
//! on the stored journal whenever the store holds another than it expects (edits recorded
//! before the load, another history's saves over the same store), so no writer's entries are
//! lost.

use std::sync::{Arc, MutexGuard, PoisonError};

use futures::FutureExt as _;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use rayx_app::services::history::{HistoryEntry, HistoryError, HistoryScopeId};

use crate::journal::HistoryJournalV2;
use crate::service::{RayxHistory, ScopeHistory, Scopes};
use crate::store::{HistoryStore, HistoryStoreError};

/// How many times a save that meets another journal rebases on it and saves again.
const SAVE_ATTEMPTS: usize = 3;

/// Puts what `scope` changed since its base (the journal it last loaded or saved) on `stored`:
/// the stored entries, less those the scope dropped since (undone, cleared, evicted), then the
/// entries it recorded since. Its base is `stored` from then on.
pub(crate) fn rebase(scope: &mut ScopeHistory, stored: Option<HistoryJournalV2>) {
    let stored_revision = stored.as_ref().map(|journal| journal.revision);
    let stored_entries = stored.map(|journal| journal.entries).unwrap_or_default();
    let ours = std::mem::take(&mut scope.undo);
    let dropped: Vec<&HistoryEntry> = scope
        .base
        .iter()
        .filter(|entry| !ours.contains(entry))
        .collect();
    let mut undo: Vec<HistoryEntry> = stored_entries
        .iter()
        .filter(|entry| !dropped.contains(entry))
        .cloned()
        .collect();
    for entry in ours {
        if !scope.base.contains(&entry) && !undo.contains(&entry) {
            undo.push(entry);
        }
    }
    let differs = undo != stored_entries;
    scope.undo = undo;
    scope.base = stored_entries;
    scope.persisted_revision = stored_revision;
    let floor = stored_revision.unwrap_or(0);
    // Past the stored revision when it differs from it, so it is saved.
    scope.revision = scope.revision.max(floor) + u64::from(differs);
    if differs {
        scope.enforce_entry_limit();
    }
}

impl RayxHistory {
    /// The service's state, even after a thread panicked holding it.
    fn state(&self) -> MutexGuard<'_, Scopes> {
        self.scopes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The history's store, if it has one.
    fn current_store(&self) -> Option<Arc<dyn HistoryStore>> {
        self.store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
        let journal = match self.current_store() {
            Some(store) => self.bounded(store.load(scope_id)).await?,
            None if self.store_configured => return Ok(()),
            None => None,
        };
        self.merge_loaded(scope_id, journal)?;
        self.publish_state();
        Ok(())
    }

    /// Puts what the scope recorded since its start on the loaded `journal` (unless it was
    /// loaded meanwhile).
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
        rebase(scope, journal);
        scope.loaded = true;
        Ok(())
    }

    /// Saves the scope behind its change; the outcome of the save that holds it. One writer,
    /// detached, saves at a time, each scope at its latest revision; dropping the returned
    /// future gives up only the wait (a failure nobody hears then is a failed step of the
    /// scope). Before the app's store is attached, the scope is left unsaved, saved once it
    /// is, and this answers at once.
    pub(crate) fn persist_scope(
        &self,
        scope_id: &HistoryScopeId,
    ) -> BoxFuture<'static, Result<(), HistoryError>> {
        if !self.store_configured || self.current_store().is_none() {
            self.persist(scope_id, None);
            return futures::future::ready(Ok(())).boxed();
        }
        let (answer, answered) = oneshot::channel();
        self.persist(scope_id, Some(answer));
        // A writer that ended without answering saved nothing more.
        async move { answered.await.unwrap_or(Ok(())) }.boxed()
    }

    /// Saves `scope_id` where nothing awaits it: at once with a store that answers at once,
    /// else when the store answers; a failure is a failed step of the scope.
    pub(crate) fn persist_behind(&self, scope_id: &HistoryScopeId) {
        self.persist(scope_id, None);
    }

    /// Marks `scope_id` unsaved (`waiter` hearing its next save) and starts the writer when
    /// the store is attached and none runs.
    fn persist(
        &self,
        scope_id: &HistoryScopeId,
        waiter: Option<oneshot::Sender<Result<(), HistoryError>>>,
    ) {
        if !self.store_configured {
            return;
        }
        let attached = self.current_store().is_some();
        let start = {
            let mut state = self.state();
            state.unsaved.insert(scope_id.clone());
            if let Some(waiter) = waiter {
                state
                    .save_waiters
                    .entry(scope_id.clone())
                    .or_default()
                    .push(waiter);
            }
            attached && !std::mem::replace(&mut state.writing, true)
        };
        if start {
            self.start_writer();
        }
    }

    /// Starts the writer for what was changed before the app's store was attached.
    pub(crate) fn save_unsaved(&self) {
        let start = {
            let mut state = self.state();
            !state.unsaved.is_empty() && !std::mem::replace(&mut state.writing, true)
        };
        if start {
            self.start_writer();
        }
    }

    /// Runs the writer detached (its turn taken already).
    fn start_writer(&self) {
        let Some(history) = self.this.upgrade() else {
            self.state().writing = false;
            return;
        };
        crate::detached::spawn(Box::pin(async move { history.write().await }));
    }

    /// Saves the unsaved scopes until none is left, answering each one's waiters; a failure
    /// none of them hears is a failed step of the scope.
    async fn write(self: Arc<Self>) {
        let Some(store) = self.current_store() else {
            self.state().writing = false;
            return;
        };
        loop {
            let (next, waiters) = {
                let mut state = self.state();
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
            let mut heard = false;
            for waiter in waiters {
                heard |= waiter.send(saved.clone()).is_ok();
            }
            if !heard && let Err(error) = &saved {
                self.record_failure(next, error);
            }
        }
    }

    /// Saves `scope_id` at its revision now (loading it first if it was not); a save that
    /// meets another journal in the store reloads it, rebases the scope's changes on it and
    /// saves again (unless it holds this very journal, a save whose answer was lost).
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
            let journal = self.bounded(store.load(scope_id)).await?;
            self.merge_loaded(scope_id, journal)?;
            self.publish_state();
        }
        let mut attempt = 1;
        loop {
            let (journal, expected_revision) = self.persistence_snapshot(scope_id)?;
            if expected_revision == Some(journal.revision) {
                return Ok(());
            }
            match self
                .bounded_store(store.save(journal.clone(), expected_revision))
                .await?
            {
                Ok(()) => return self.record_saved(scope_id, journal),
                Err(HistoryStoreError::Conflict) if attempt < SAVE_ATTEMPTS => {
                    attempt += 1;
                    let stored = self.bounded(store.load(scope_id)).await?;
                    if stored.as_ref() == Some(&journal) {
                        return self.record_saved(scope_id, journal);
                    }
                    {
                        let mut state = self.lock_state()?;
                        let scope = state
                            .scopes
                            .get_mut(scope_id)
                            .ok_or(HistoryError::StackChanged)?;
                        rebase(scope, stored);
                    }
                    self.publish_state();
                }
                Err(error) => return Err(error.into()),
            }
        }
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

    /// `journal` is the scope's in the store: its base from now on.
    fn record_saved(
        &self,
        scope_id: &HistoryScopeId,
        journal: HistoryJournalV2,
    ) -> Result<(), HistoryError> {
        let mut state = self.lock_state()?;
        let scope = state
            .scopes
            .get_mut(scope_id)
            .ok_or(HistoryError::StackChanged)?;
        scope.persisted_revision = Some(journal.revision);
        scope.base = journal.entries;
        Ok(())
    }
}
