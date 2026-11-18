//! The records a scope retains.

use rayx_app::services::history::{HistoryError, HistoryScopeId};
use rayx_app::services::operations::OperationRecord;

use crate::service::RayxHistory;

impl RayxHistory {
    /// Snapshots the currently retained semantic records, newest undo entries
    /// first, followed by newest redo entries. Each entry contributes its inverse
    /// records then forward records in stored order. Immutable payload bytes are
    /// shared; no callbacks run while the history state is locked.
    ///
    /// The snapshot does not change history. A caller may inspect domain-owned
    /// resources without maintaining a second registry; release the snapshot when
    /// the inspection finishes so evicted payloads can be reclaimed.
    pub(crate) async fn retained_records_now(
        &self,
        scope_id: &HistoryScopeId,
    ) -> Result<Vec<OperationRecord>, HistoryError> {
        self.ensure_scope_loaded(scope_id).await?;
        let state = self.lock_state()?;
        let Some(scope) = state.scopes.get(scope_id) else {
            return Ok(Vec::new());
        };
        Ok(scope
            .undo
            .iter()
            .rev()
            .chain(scope.redo.iter().rev())
            .flat_map(|entry| entry.inverse.iter().chain(&entry.forward))
            .cloned()
            .collect())
    }
}
