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
/// registers one, `add_persistent_history` the app's key-value store's). It answers
/// synchronously, and is a startup service keyed by `dyn HistoryStore` the history waits for.
pub trait HistoryStore: StartupInitializable {
    fn load(&self, scope: &HistoryScopeId) -> Result<Option<HistoryJournalV2>, HistoryStoreError>;
    fn save(
        &self,
        journal: HistoryJournalV2,
        expected_revision: Option<u64>,
    ) -> Result<(), HistoryStoreError>;
    fn delete_scope(&self, scope: &HistoryScopeId) -> Result<(), HistoryStoreError>;

    fn load_async<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
        async move { self.load(scope) }.boxed()
    }

    fn save_async(
        &self,
        journal: HistoryJournalV2,
        expected_revision: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
        async move { self.save(journal, expected_revision) }.boxed()
    }

    fn delete_scope_async<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>> {
        async move { self.delete_scope(scope) }.boxed()
    }
}

/// A [`HistoryStore`] in memory, for the app's life (and tests).
#[derive(Clone, Default)]
pub struct InMemoryHistoryStore {
    journals: Arc<Mutex<BTreeMap<HistoryScopeId, HistoryJournalV2>>>,
    initialization: StartupInitializationState,
}

impl HistoryStore for InMemoryHistoryStore {
    fn load(&self, scope: &HistoryScopeId) -> Result<Option<HistoryJournalV2>, HistoryStoreError> {
        Ok(self
            .journals
            .lock()
            .map_err(|_| HistoryStoreError::Poisoned)?
            .get(scope)
            .cloned())
    }
    fn save(
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
    fn delete_scope(&self, scope: &HistoryScopeId) -> Result<(), HistoryStoreError> {
        self.journals
            .lock()
            .map_err(|_| HistoryStoreError::Poisoned)?
            .remove(scope);
        Ok(())
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
