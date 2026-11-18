//! A [`HistoryStore`] written in TS: the contract [`HistoryJournalStore`] (TS's
//! `HistoryStoreContract`) a TS class implements, keeping each scope's journal as JSON text, and
//! the [`HistoryStore`] RayX's history keeps its journals in over it
//! (`builder.addHistoryWithStore(() => new MyStore())`).

use std::sync::Arc;

use futures::FutureExt as _;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceType,
};
use rayx_app::services::history::HistoryScopeId;
use rayx_app_nsh::ts::{ContractError, Exception};

use crate::journal::HistoryJournalV2;
use crate::store::{HistoryStore, HistoryStoreError};

impl From<ContractError> for HistoryStoreError {
    fn from(error: ContractError) -> Self {
        Self::Backend(error.to_string())
    }
}

impl From<Exception> for HistoryStoreError {
    fn from(exception: Exception) -> Self {
        Self::Backend(exception.message)
    }
}

/// Where a history store written in TS keeps each scope's journal: its JSON text, saved whole.
/// TS implements it as `HistoryStoreContract` and registers it with
/// `builder.addHistoryWithStore(() => new MyStore())`; RayX's history keeps its journals in it
/// through a [`HistoryStore`] over it. What the TS methods throw is the store's
/// [`HistoryStoreError::Backend`].
#[rayx_app_nsh::contract(ts = "HistoryStoreContract")]
pub trait HistoryJournalStore {
    /// The journal saved for `scope`, as the JSON text `save` was given; `null` when there is
    /// none.
    async fn load(&self, scope: String) -> Result<Option<String>, HistoryStoreError>;

    /// Saves `journal` (JSON text, at `revision`) for `scope` when the saved journal is at
    /// `expected_revision` (`null`: none is saved); whether it did (`false`: another revision is
    /// saved, a conflict).
    async fn save(
        &self,
        scope: String,
        journal: String,
        revision: f64,
        expected_revision: Option<f64>,
    ) -> Result<bool, HistoryStoreError>;

    /// Forgets the journal of `scope`.
    async fn delete_scope(&self, scope: String) -> Result<(), HistoryStoreError>;
}

/// RayX's [`HistoryStore`] over the app's `dyn HistoryJournalStore`: journals cross as JSON
/// text. It is initialized at once (the TS store is made by `start()` before it).
pub(crate) struct JournalStore(pub(crate) Arc<dyn HistoryJournalStore>);

impl HistoryStore for JournalStore {
    fn load<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<Option<HistoryJournalV2>, HistoryStoreError>> {
        Box::pin(async move {
            match self.0.load(scope.as_str().to_owned()).await? {
                None => Ok(None),
                Some(text) => serde_json::from_str(&text).map(Some).map_err(|error| {
                    HistoryStoreError::Backend(format!("the journal of {scope:?}: {error}"))
                }),
            }
        })
    }

    fn save(
        &self,
        journal: HistoryJournalV2,
        expected_revision: Option<u64>,
    ) -> BoxFuture<'_, Result<(), HistoryStoreError>> {
        Box::pin(async move {
            let text = serde_json::to_string(&journal)
                .map_err(|error| HistoryStoreError::Backend(error.to_string()))?;
            let saved = self
                .0
                .save(
                    journal.scope_id.as_str().to_owned(),
                    text,
                    journal.revision as f64,
                    expected_revision.map(|revision| revision as f64),
                )
                .await?;
            if saved {
                Ok(())
            } else {
                Err(HistoryStoreError::Conflict)
            }
        })
    }

    fn delete_scope<'a>(
        &'a self,
        scope: &'a HistoryScopeId,
    ) -> BoxFuture<'a, Result<(), HistoryStoreError>> {
        Box::pin(async move { self.0.delete_scope(scope.as_str().to_owned()).await })
    }
}

impl AsyncInitializable for JournalStore {
    fn is_initialized(&self) -> bool {
        true
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        futures::future::ready(Ok(())).boxed_local()
    }
}

impl StartupInitializable for JournalStore {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn HistoryStore>())
    }
}
