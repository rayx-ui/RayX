use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::HistoryError;
use crate::services::operations::{OperationRecord, OperationTargetId};

/// A history scope's id: a window, a document, an inspector (1 to 256 bytes, not blank).
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct HistoryScopeId(String);

impl HistoryScopeId {
    /// The id `value`, or [`HistoryError::InvalidScopeId`] when it is blank or longer than 256
    /// bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, HistoryError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 {
            return Err(HistoryError::InvalidScopeId);
        }
        Ok(Self(value))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One undoable step: the operations of one transaction on one target, with their inverses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryEntry {
    /// The entry's identity, given when it is recorded and kept with its journal: unique
    /// across histories and app runs, so histories sharing a store tell their entries apart (an
    /// entry of a journal saved before entries had ids gets one when it is loaded).
    #[serde(default)]
    pub id: String,
    pub transaction_id: u64,
    pub scope_id: HistoryScopeId,
    pub target_id: OperationTargetId,
    pub label_key: String,
    pub forward: Vec<OperationRecord>,
    pub inverse: Vec<OperationRecord>,
    pub before_revision: u64,
    pub after_revision: u64,
}

/// An undo or redo that failed, in its scope, with the error's code.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryNavigationFailure {
    pub scope_id: HistoryScopeId,
    pub error_code: String,
}

/// What a history service publishes as each step happens.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HistoryEvent {
    /// A new entry (a transaction's later operations join it without an event).
    Recorded(HistoryEntry),
    ScopeCleared(HistoryScopeId),
    /// The scope's entry limit changed (entries may have been evicted).
    AvailabilityChanged(HistoryScopeId),
    UndoCompleted(HistoryScopeId),
    RedoCompleted(HistoryScopeId),
    NavigationFailed(HistoryNavigationFailure),
}

/// A scope's stacks, as a history service publishes them.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryScopeState {
    /// How many entries undo can replay.
    pub undo_depth: usize,
    /// How many entries redo can replay.
    pub redo_depth: usize,
    /// Changes with every change of the scope's stacks (and each failed replay).
    pub revision: u64,
    /// The most entries the scope keeps, when it is bounded.
    pub entry_limit: Option<usize>,
    /// Label key of the next entry to undo (top of the undo stack), if any.
    #[serde(default)]
    pub undo_label: Option<String>,
    /// Label key of the next entry to redo (top of the redo stack), if any.
    #[serde(default)]
    pub redo_label: Option<String>,
}

/// What a history service publishes as its state: each scope it has loaded or recorded in, and
/// what its diagnostics keep (RayX's with its `diagnostics` feature).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryState {
    /// The scopes, by id; a scope not here has empty stacks (or is not loaded yet).
    pub scopes: BTreeMap<HistoryScopeId, HistoryScopeState>,
    /// How many replays failed because their target was released.
    #[serde(default)]
    pub unavailable_entry_count: usize,
    /// The latest failures' codes, oldest first.
    #[serde(default)]
    pub recent_failure_codes: Vec<String>,
}

impl HistoryState {
    /// The stacks of `scope_id` (empty for a scope it does not have).
    pub fn scope(&self, scope_id: &HistoryScopeId) -> HistoryScopeState {
        self.scopes.get(scope_id).cloned().unwrap_or_default()
    }

    /// Whether `scope_id` has an entry to undo.
    pub fn can_undo(&self, scope_id: &HistoryScopeId) -> bool {
        self.scope(scope_id).undo_depth > 0
    }

    /// Whether `scope_id` has an entry to redo.
    pub fn can_redo(&self, scope_id: &HistoryScopeId) -> bool {
        self.scope(scope_id).redo_depth > 0
    }

    /// The undo and redo depths of `scope_id`.
    pub fn stack_depths(&self, scope_id: &HistoryScopeId) -> (usize, usize) {
        let scope = self.scope(scope_id);
        (scope.undo_depth, scope.redo_depth)
    }

    /// Label key of the next entry to undo in `scope_id`, if any.
    pub fn undo_label(&self, scope_id: &HistoryScopeId) -> Option<String> {
        self.scopes
            .get(scope_id)
            .and_then(|scope| scope.undo_label.clone())
    }

    /// Label key of the next entry to redo in `scope_id`, if any.
    pub fn redo_label(&self, scope_id: &HistoryScopeId) -> Option<String> {
        self.scopes
            .get(scope_id)
            .and_then(|scope| scope.redo_label.clone())
    }
}

/// A read-only view of a history scope (`diagnostics(scope)` on `dyn HistoryService`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryDiagnostics {
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub stack_revision: u64,
    pub subscriber_count: usize,
    pub unavailable_entry_count: usize,
    pub recent_failure_codes: Vec<String>,
}
