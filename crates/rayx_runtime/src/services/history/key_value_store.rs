use super::{HistoryJournalV2, HistoryScopeId, HistoryStore, HistoryStoreError};
use crate::services::storage::KeyValueStore;
use futures::{FutureExt as _, future::LocalBoxFuture};
use rayx_core::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use std::sync::{Arc, Mutex};

/// The bucket of the app's [`KeyValueStore`] that [`KeyValueHistoryStore`] keeps journals in.
pub const HISTORY_BUCKET: &str = "rayx.history";

/// A [`HistoryStore`] over a [`KeyValueStore`]: each scope's journal is one JSON value in
/// [`HISTORY_BUCKET`], keyed by the scope's id, so history persists wherever the store keeps its
/// values (NSH's storage on a device, through `rayx_app`).
#[derive(Clone)]
pub struct KeyValueHistoryStore {
    store: Arc<dyn KeyValueStore>,
    /// Holds a save's revision check and its write together.
    saving: Arc<Mutex<()>>,
    initialization: StartupInitializationState,
}

impl KeyValueHistoryStore {
    /// Journals kept in `store`.
    pub fn new(store: Arc<dyn KeyValueStore>) -> Self {
        Self {
            store,
            saving: Arc::default(),
            initialization: StartupInitializationState::default(),
        }
    }
}

fn backend(error: impl std::fmt::Display) -> HistoryStoreError {
    HistoryStoreError::Backend(error.to_string())
}

impl HistoryStore for KeyValueHistoryStore {
    fn load(&self, scope: &HistoryScopeId) -> Result<Option<HistoryJournalV2>, HistoryStoreError> {
        self.store
            .get(HISTORY_BUCKET, scope.as_str())
            .map_err(backend)?
            .map(|bytes| {
                HistoryJournalV2::decode(&bytes).map_err(|error| {
                    backend(format!("the journal of {} is {error:?}", scope.as_str()))
                })
            })
            .transpose()
    }

    fn save(
        &self,
        journal: HistoryJournalV2,
        expected: Option<u64>,
    ) -> Result<(), HistoryStoreError> {
        let _saving = self
            .saving
            .lock()
            .map_err(|_| HistoryStoreError::Poisoned)?;
        let current = self
            .load(&journal.scope_id)?
            .map(|journal| journal.revision);
        if current != expected {
            return Err(HistoryStoreError::Conflict);
        }
        let bytes = journal
            .encode()
            .map_err(|error| backend(format!("the journal did not encode: {error:?}")))?;
        self.store
            .set(HISTORY_BUCKET, journal.scope_id.as_str(), bytes)
            .map_err(backend)
    }

    fn delete_scope(&self, scope: &HistoryScopeId) -> Result<(), HistoryStoreError> {
        self.store
            .remove(HISTORY_BUCKET, scope.as_str())
            .map(|_| ())
            .map_err(backend)
    }
}

impl AsyncInitializable for KeyValueHistoryStore {
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

impl StartupInitializable for KeyValueHistoryStore {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<dyn HistoryStore>()
    }
}
