//! Loading a scope from the history's store and saving it there, asynchronously: a scope is
//! loaded the first time a command uses it, and saved behind each change by one writer at a
//! time, which runs detached: a command awaits only the outcome of its save, so a command given
//! up on leaves no save half done. The journal saved keeps the entries other histories saved
//! in the same store (their sessions undo them, never this one): when the store holds another
//! journal than this history expects, it reloads it and merges its own entries in by id, so no
//! writer's entries are lost. A cleared scope whose journal has no entry left (no other
//! history's) is not saved: it is deleted from the store (`delete_scope`), unless another
//! journal is there now.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, MutexGuard, PoisonError};

use futures::FutureExt as _;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use rayx_app::services::history::{HistoryEntry, HistoryError, HistoryScopeId};

use crate::journal::HistoryJournalV2;
use crate::service::{RayxHistory, ScopeHistory, Scopes};
use crate::store::{HistoryStore, HistoryStoreError};

/// How many times a save that meets another journal merges with it and saves again.
const SAVE_ATTEMPTS: usize = 3;

/// `entries` with an id each: an entry of a journal written before entries had ids gets one
/// from its place and transaction (the same in every history that loads that journal).
pub(crate) fn with_ids(mut entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    for (index, entry) in entries.iter_mut().enumerate() {
        if entry.id.is_empty() {
            entry.id = format!("legacy:{index}:{}", entry.transaction_id);
        }
    }
    entries
}

/// The journal to save for `scope`: the stored entries (another history's included) less the
/// ones this history saved and dropped since (undone, cleared, evicted), each of its own
/// replaced by its current copy, then its entries it never saved. Matched by id; the scope's
/// live stacks are not changed.
pub(crate) fn journal_entries(scope: &ScopeHistory) -> Vec<HistoryEntry> {
    let ours: BTreeMap<&str, &HistoryEntry> = scope
        .undo
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();
    let mut entries = Vec::with_capacity(scope.stored.len() + scope.undo.len());
    let mut included = BTreeSet::new();
    for stored in &scope.stored {
        let own = ours.get(stored.id.as_str());
        if own.is_none() && scope.saved_own.contains(&stored.id) {
            continue;
        }
        if included.insert(stored.id.clone()) {
            entries.push((*own.unwrap_or(&stored)).clone());
        }
    }
    // Only entries this history never saved: one it saved that the store no longer holds was
    // removed by another writer (undone, cleared), and stays out.
    for entry in &scope.undo {
        if !scope.saved_own.contains(&entry.id) && included.insert(entry.id.clone()) {
            entries.push(entry.clone());
        }
    }
    entries
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

    /// Puts the loaded `journal` (this history's own from now on) under what the scope
    /// recorded since its start (unless it was loaded meanwhile).
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
            let loaded = with_ids(journal.entries);
            let recorded = std::mem::replace(&mut scope.undo, loaded.clone());
            let had_recorded = !recorded.is_empty();
            for entry in recorded {
                if !scope.undo.iter().any(|loaded| loaded.id == entry.id) {
                    scope.undo.push(entry);
                }
            }
            scope.saved_own = loaded.iter().map(|entry| entry.id.clone()).collect();
            scope.stored = loaded;
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
    /// meets another journal in the store reloads it, merges this history's entries in and
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
            let (journal, expected_revision, own, cleared) = self.persistence_snapshot(scope_id)?;
            if expected_revision == Some(journal.revision) {
                return Ok(());
            }
            if cleared && journal.entries.is_empty() {
                // Nothing left to keep (a cleared scope, no other history's entries in it):
                // the scope is forgotten in the store, unless another journal is there now.
                let Some(expected_revision) = expected_revision else {
                    return Ok(());
                };
                let stored = self.bounded(store.load(scope_id)).await?;
                match stored.as_ref().map(|stored| stored.revision) {
                    None => return self.record_deleted(scope_id),
                    Some(revision) if revision == expected_revision => {
                        self.bounded(store.delete_scope(scope_id)).await?;
                        return self.record_deleted(scope_id);
                    }
                    Some(_) if attempt < SAVE_ATTEMPTS => {
                        attempt += 1;
                        self.take_stored(scope_id, stored)?;
                        continue;
                    }
                    Some(_) => return Err(HistoryStoreError::Conflict.into()),
                }
            }
            match self
                .bounded_store(store.save(journal.clone(), expected_revision))
                .await?
            {
                Ok(()) => return self.record_saved(scope_id, journal, own),
                Err(HistoryStoreError::Conflict) if attempt < SAVE_ATTEMPTS => {
                    attempt += 1;
                    let stored = self.bounded(store.load(scope_id)).await?;
                    if stored.as_ref() == Some(&journal) {
                        return self.record_saved(scope_id, journal, own);
                    }
                    self.take_stored(scope_id, stored)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// The journal to save, the revision expected in the store and the ids of this history's
    /// entries in it.
    fn persistence_snapshot(
        &self,
        scope_id: &HistoryScopeId,
    ) -> Result<(HistoryJournalV2, Option<u64>, BTreeSet<String>, bool), HistoryError> {
        let state = self.lock_state()?;
        let scope = state
            .scopes
            .get(scope_id)
            .ok_or(HistoryError::StackChanged)?;
        Ok((
            HistoryJournalV2::new(scope.revision, scope_id.clone(), journal_entries(scope)),
            scope.persisted_revision,
            scope.undo.iter().map(|entry| entry.id.clone()).collect(),
            scope.cleared,
        ))
    }

    /// `stored` is the scope's journal in the store: merged with from now on, at a revision
    /// past it.
    fn take_stored(
        &self,
        scope_id: &HistoryScopeId,
        stored: Option<HistoryJournalV2>,
    ) -> Result<(), HistoryError> {
        {
            let mut state = self.lock_state()?;
            let scope = state
                .scopes
                .get_mut(scope_id)
                .ok_or(HistoryError::StackChanged)?;
            let revision = stored.as_ref().map(|journal| journal.revision);
            scope.stored = stored
                .map(|journal| with_ids(journal.entries))
                .unwrap_or_default();
            scope.persisted_revision = revision;
            scope.revision = scope.revision.max(revision.unwrap_or(0)) + 1;
        }
        self.publish_state();
        Ok(())
    }

    /// The scope has no journal in the store (this history deleted it, or found it gone).
    fn record_deleted(&self, scope_id: &HistoryScopeId) -> Result<(), HistoryError> {
        let mut state = self.lock_state()?;
        let scope = state
            .scopes
            .get_mut(scope_id)
            .ok_or(HistoryError::StackChanged)?;
        scope.persisted_revision = None;
        scope.stored.clear();
        scope.saved_own.clear();
        scope.cleared = false;
        Ok(())
    }

    /// `journal` is the scope's in the store, with this history's entries `own`.
    fn record_saved(
        &self,
        scope_id: &HistoryScopeId,
        journal: HistoryJournalV2,
        own: BTreeSet<String>,
    ) -> Result<(), HistoryError> {
        let mut state = self.lock_state()?;
        let scope = state
            .scopes
            .get_mut(scope_id)
            .ok_or(HistoryError::StackChanged)?;
        // A clear made after this journal was taken is still to be saved.
        if journal.revision == scope.revision {
            scope.cleared = false;
        }
        scope.persisted_revision = Some(journal.revision);
        scope.stored = journal.entries;
        scope.saved_own = own;
        Ok(())
    }
}
