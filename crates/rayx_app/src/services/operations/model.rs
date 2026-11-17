use serde::{Deserialize, Serialize};

use super::{OperationRecord, OperationTargetId};

/// Where an operation comes from: history records only the local user's.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperationOrigin {
    LocalUser,
    System,
    Import,
    HistoryUndo,
    HistoryRedo,
    /// A peer's, by its id.
    Remote(String),
}

impl OperationOrigin {
    /// Whether it is the local user's.
    pub fn is_local_user(&self) -> bool {
        matches!(self, Self::LocalUser)
    }
}

/// What applying an operation did.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperationApplyStatus {
    Applied,
    NoOp,
    Conflict,
}

/// What a target's handler answers for an operation of `T`: its status, the inverse (undoing
/// it), the ids it changed and the target's revisions before and after.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationApplyResult<T> {
    pub status: OperationApplyStatus,
    pub inverse: Option<T>,
    pub affected_ids: Vec<String>,
    pub before_revision: u64,
    pub after_revision: u64,
}

impl<T> OperationApplyResult<T> {
    /// Applied, undone by `inverse`.
    pub fn applied(
        inverse: T,
        affected_ids: impl IntoIterator<Item = impl Into<String>>,
        before_revision: u64,
        after_revision: u64,
    ) -> Self {
        Self {
            status: OperationApplyStatus::Applied,
            inverse: Some(inverse),
            affected_ids: affected_ids.into_iter().map(Into::into).collect(),
            before_revision,
            after_revision,
        }
    }

    /// Nothing changed (the target was there already).
    pub fn no_op(revision: u64) -> Self {
        Self {
            status: OperationApplyStatus::NoOp,
            inverse: None,
            affected_ids: Vec::new(),
            before_revision: revision,
            after_revision: revision,
        }
    }

    /// The operation conflicts with the target's state; nothing changed.
    pub fn conflict(revision: u64) -> Self {
        Self {
            status: OperationApplyStatus::Conflict,
            inverse: None,
            affected_ids: Vec::new(),
            before_revision: revision,
            after_revision: revision,
        }
    }
}

/// An operation a target applied: in its transaction, with its inverse record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationReceipt {
    pub transaction_id: u64,
    pub target_id: OperationTargetId,
    pub forward: OperationRecord,
    pub inverse: Option<OperationRecord>,
    pub origin: OperationOrigin,
    pub status: OperationApplyStatus,
    pub affected_ids: Vec<String>,
    pub before_revision: u64,
    pub after_revision: u64,
}

/// An operation a target refused or failed to apply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationFailure {
    pub transaction_id: u64,
    pub target_id: OperationTargetId,
    pub kind: String,
    pub version: u32,
    pub origin: OperationOrigin,
    pub error_code: String,
}

/// What an operation service publishes for each operation: committed (outside a transaction at
/// once, inside one when it commits) or failed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperationEvent {
    Committed(OperationReceipt),
    Failed(OperationFailure),
}

/// A committed operation, without its payload: what diagnostics keep of it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationTransactionDiagnostics {
    pub transaction_id: u64,
    pub target_id: OperationTargetId,
    pub kind: String,
    pub version: u32,
    pub status: OperationApplyStatus,
    pub origin: OperationOrigin,
    pub before_revision: u64,
    pub after_revision: u64,
}

impl OperationTransactionDiagnostics {
    /// Whether it carries an operation's values: never (diagnostics are redacted).
    pub fn contains_payload_values(&self) -> bool {
        false
    }
}

/// What an operation service publishes as its state: the targets registered now and, when the
/// implementation keeps them (RayX's with its `diagnostics` feature), the latest committed
/// operations and failure codes.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationState {
    /// The registered targets, sorted.
    pub targets: Vec<OperationTargetId>,
    /// The latest committed operations, oldest first.
    #[serde(default)]
    pub recent_transactions: Vec<OperationTransactionDiagnostics>,
    /// The latest failures' codes, oldest first.
    #[serde(default)]
    pub recent_failure_codes: Vec<String>,
}

impl OperationState {
    /// Whether a target of `target_id` is registered.
    pub fn has_target(&self, target_id: &OperationTargetId) -> bool {
        self.targets.binary_search(target_id).is_ok()
    }
}

/// A read-only view of an operation service: its targets, its subscribers and its latest
/// operations (`diagnostics()` on `dyn OperationService`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDiagnostics {
    pub registered_target_count: usize,
    pub subscriber_count: usize,
    pub recent_transactions: Vec<OperationTransactionDiagnostics>,
    pub recent_failure_codes: Vec<String>,
}
