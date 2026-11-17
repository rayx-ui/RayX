use crate::journal::HistoryJournalV2;
use futures::{
    FutureExt as _,
    future::{BoxFuture, LocalBoxFuture},
};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::history::{HistoryError, HistoryScopeId};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HistoryStoreError {
    #[error("history store revision conflict")]
    Conflict,
    #[error("history store backend error: {0}")]
    Backend(String),
    #[error("history store state is poisoned")]
    Poisoned,
}

impl From<HistoryStoreError> for HistoryError {
    fn from(error: HistoryStoreError) -> Self {
        Self::Store(error.to_string())
    }
}

/// Where RayX's history keeps each scope's journal: its extension point (`add_history_with_store`
/// registers one, `add_persistent_history` the app's key-value store's). Its calls answer
/// asynchronously, at once or later (a store over a TS `KeyValueStore` answers in JS's next
/// turn): the history loads a scope when it is first used and saves it behind its changes, one
/// save at a time. It is a startup service keyed by `dyn HistoryStore` the history waits for.
pub trait HistoryStore: StartupInitializable {
    /// The journal of `scope`, if one was saved.
    fn load<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>>;

    /// Saves `journal` when the saved one is at `expected_revision` (`None`: there is none),
    /// else fails with [`HistoryStoreError::Conflict`].
    fn save(
        &self,
        journal: HistoryJournalV2,
        expected_revision: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>>;

    /// Forgets the journal of `scope`.
    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>>;
}

/// A [`HistoryStore`] in memory, for the app's life (and tests); it answers at once.
#[derive(Clone, Default)]
pub struct InMemoryHistoryStore {
    journals: Arc<Mutex<BTreeMap<HistoryScopeId, HistoryJournalV2>>>,
    initialization: StartupInitializationState,
}

impl InMemoryHistoryStore {
    fn load_now(
        &self,
        scope: &HistoryScopeId,
    ) -> Result<Option<HistoryJournalV2>, HistoryStoreError> {
        Ok(self
            .journals
            .lock()
            .map_err(|_| HistoryStoreError::Poisoned)?
            .get(scope)
            .cloned())
    }

    fn save_now(
        &self,
        journal: HistoryJournalV2,
        expected: Option<u64>,
    ) -> Result<(), HistoryStoreError> {
        let mut journals = self
            .journals
            .lock()
            .map_err(|_| HistoryStoreError::Poisoned)?;
        if journals.get(&journal.scope_id).map(|j| j.revision) != expected {
            return Err(HistoryStoreError::Conflict);
        }
        journals.insert(journal.scope_id.clone(), journal);
        Ok(())
    }

    fn delete_now(&self, scope: &HistoryScopeId) -> Result<(), HistoryStoreError> {
        self.journals
            .lock()
            .map_err(|_| HistoryStoreError::Poisoned)?
            .remove(scope);
        Ok(())
    }
}

impl HistoryStore for InMemoryHistoryStore {
    fn load<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
        futures::future::ready(self.load_now(scope)).boxed()
    }

    fn save(
        &self,
        journal: HistoryJournalV2,
        expected: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
        futures::future::ready(self.save_now(journal, expected)).boxed()
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>> {
        futures::future::ready(self.delete_now(scope)).boxed()
    }
}

impl AsyncInitializable for InMemoryHistoryStore {
    fn is_initialized(&self) -> bool {
        self.initialization.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialization = self.initialization.clone();
        async move {
            initialization.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for InMemoryHistoryStore {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }
}
