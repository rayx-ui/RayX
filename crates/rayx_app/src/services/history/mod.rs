//! History: undo and redo per scope (a window, a document), of the operations the local user
//! committed through the app's `dyn OperationService` (each transaction one entry), published by
//! a [`HistoryService`] implementation as [`HistoryState`] (each scope's depths) and
//! [`HistoryEvent`]s.
//!
//! RayX's implementation is `rayx_services_history`'s (`add_history()`, kept in memory, or
//! `add_persistent_history()`, in the app's key-value store); an app may register its own, in
//! Rust or in TS. Reading the state (`can_undo`, `can_redo`, `stack_depths`) is synchronous for
//! any implementation, and so are the same helpers on `dyn HistoryService`.

mod error;
mod model;

pub use error::HistoryError;
pub use model::{
    HistoryDiagnostics, HistoryEntry, HistoryEvent, HistoryNavigationFailure, HistoryScopeId,
    HistoryScopeState, HistoryState,
};

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::ChangeSubscription;
use crate::published::{EventStream, StateCell};
use crate::services::operations::{OperationRecord, OperationTargetId};

/// The history: what the local user did, per scope, undone and redone by resubmitting the
/// recorded operations' inverses (or the operations) to the app's operation service. Its state
/// is each scope's depths, its events each step. A target's operations are recorded in the
/// scope it is bound to, else in the scope of its id.
#[rayx_app_macros::contract(ts = "HistoryServiceContract")]
pub trait HistoryService {
    /// Each scope's depths, now and as they change.
    #[state(ts = "HistoryState")]
    fn history(&self) -> &StateCell<HistoryState>;

    /// Each entry recorded, undo and redo completed or failed, scope cleared or bounded.
    #[events(ts = "HistoryEvent")]
    fn events(&self) -> &EventStream<HistoryEvent>;

    /// Records the operations of `target_id` in `scope_id` from now on, and loads the scope's
    /// history (a persisted one, from its store).
    #[ts("(targetId: string, scopeId: string)")]
    async fn bind_target_scope(
        &self,
        target_id: OperationTargetId,
        scope_id: HistoryScopeId,
    ) -> Result<(), HistoryError>;

    /// Undoes the scope's latest entry; whether there was one.
    #[ts("(scopeId: string): boolean")]
    async fn undo(&self, scope_id: HistoryScopeId) -> Result<bool, HistoryError>;

    /// Redoes the scope's latest undone entry; whether there was one.
    #[ts("(scopeId: string): boolean")]
    async fn redo(&self, scope_id: HistoryScopeId) -> Result<bool, HistoryError>;

    /// Clears both of the scope's stacks, keeping its binding and limit: operations of a
    /// transaction still open are not recorded. Busy while the scope replays or a transaction
    /// is open on the thread.
    #[ts("(scopeId: string)")]
    async fn clear_scope(&self, scope_id: HistoryScopeId) -> Result<(), HistoryError>;

    /// Bounds the scope's entries to `limit` (at least 1): the oldest undo entries go first,
    /// then the most distant redo entries. Busy as `clear_scope` is.
    #[ts("(scopeId: string, limit: number)")]
    async fn set_scope_entry_limit(
        &self,
        scope_id: HistoryScopeId,
        limit: usize,
    ) -> Result<(), HistoryError>;

    /// The records the scope keeps: newest undo entries first, then newest redo entries, each
    /// entry's inverse records then its forward ones. A snapshot: history does not change.
    #[ts("(scopeId: string): OperationRecord[]")]
    async fn retained_records(
        &self,
        scope_id: HistoryScopeId,
    ) -> Result<Vec<OperationRecord>, HistoryError>;
}

impl dyn HistoryService {
    /// Whether `scope_id` has an entry to undo.
    pub fn can_undo(&self, scope_id: &HistoryScopeId) -> bool {
        self.history().get().can_undo(scope_id)
    }

    /// Whether `scope_id` has an entry to redo.
    pub fn can_redo(&self, scope_id: &HistoryScopeId) -> bool {
        self.history().get().can_redo(scope_id)
    }

    /// The undo and redo depths of `scope_id`.
    pub fn stack_depths(&self, scope_id: &HistoryScopeId) -> (usize, usize) {
        self.history().get().stack_depths(scope_id)
    }

    /// Calls `subscriber` with every history event, until the subscription is dropped; a
    /// subscriber that panics does not stop the others.
    pub fn subscribe(
        &self,
        subscriber: impl Fn(&HistoryEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            let _ = catch_unwind(AssertUnwindSafe(|| subscriber(event)));
        })
    }

    /// The scope's depths and revision, the subscribers and the diagnostics kept.
    pub fn diagnostics(&self, scope_id: &HistoryScopeId) -> HistoryDiagnostics {
        let state = self.history().get();
        let scope = state.scope(scope_id);
        HistoryDiagnostics {
            undo_depth: scope.undo_depth,
            redo_depth: scope.redo_depth,
            stack_revision: scope.revision,
            subscriber_count: self.events().subscribers(),
            unavailable_entry_count: state.unavailable_entry_count,
            recent_failure_codes: state.recent_failure_codes.clone(),
        }
    }
}
