use crate::journal::HistoryJournalV2;
use crate::store::{HistoryStore, HistoryStoreError};
use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupInitializationState, StartupServiceType,
};
use rayx_app::services::history::HistoryScopeId;
use rayx_app::services::storage::KeyValueStore;
use std::sync::Arc;

/// The bucket of the app's [`KeyValueStore`] that [`KeyValueHistoryStore`] keeps journals in.
pub const HISTORY_BUCKET: &str = "rayx.history";

/// A [`HistoryStore`] over the app's `dyn KeyValueStore`: each scope's journal is one JSON value
/// in [`HISTORY_BUCKET`], keyed by the scope's id, so history persists wherever the store keeps
/// its values (NSH's storage on a device, through `rayx_services_storage`, or a TS
/// implementation's, which answers in JS's next turn). It initializes after the key-value
/// store.
#[derive(Clone)]
pub struct KeyValueHistoryStore {
    store: Arc<dyn KeyValueStore>,
    /// Holds a save's or a delete's revision check and its write together, across their
    /// awaits.
    saving: Arc<futures::lock::Mutex<()>>,
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

    async fn load_journal(
        &self,
        scope: &HistoryScopeId,
    ) -> Result<Option<HistoryJournalV2>, HistoryStoreError> {
        self.store
            .get(HISTORY_BUCKET.to_owned(), scope.as_str().to_owned())
            .await
            .map_err(backend)?
            .map(|bytes| {
                HistoryJournalV2::decode(&bytes).map_err(|error| {
                    backend(format!("the journal of {} is {error:?}", scope.as_str()))
                })
            })
            .transpose()
    }
}

fn backend(error: impl std::fmt::Display) -> HistoryStoreError {
    HistoryStoreError::Backend(error.to_string())
}

impl HistoryStore for KeyValueHistoryStore {
    fn load<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
        self.load_journal(scope).boxed()
    }

    fn save(
        &self,
        journal: HistoryJournalV2,
        expected: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
        async move {
            let _saving = self.saving.lock().await;
            let current = self
                .load_journal(&journal.scope_id)
                .await?
                .map(|journal| journal.revision);
            if current != expected {
                return Err(HistoryStoreError::Conflict);
            }
            let bytes = journal
                .encode()
                .map_err(|error| backend(format!("the journal did not encode: {error:?}")))?;
            self.store
                .set(
                    HISTORY_BUCKET.to_owned(),
                    journal.scope_id.as_str().to_owned(),
                    bytes,
                )
                .await
                .map_err(backend)
        }
        .boxed()
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
        expected: u64,
    ) -> BoxFuture<'a, Result<bool, HistoryStoreError>> {
        async move {
            let _saving = self.saving.lock().await;
            let current = self
                .load_journal(scope)
                .await?
                .map(|journal| journal.revision);
            if current != Some(expected) {
                return Ok(false);
            }
            self.store
                .remove(HISTORY_BUCKET.to_owned(), scope.as_str().to_owned())
                .await
                .map(|_| true)
                .map_err(backend)
        }
        .boxed()
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
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn KeyValueStore>()]
    }
}
