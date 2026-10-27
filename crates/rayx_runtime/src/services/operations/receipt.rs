use super::{OperationOrigin, OperationRecord, OperationTargetId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationApplyStatus {
    Applied,
    NoOp,
    Conflict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationApplyResult<T> {
    pub status: OperationApplyStatus,
    pub inverse: Option<T>,
    pub affected_ids: Vec<String>,
    pub before_revision: u64,
    pub after_revision: u64,
}

impl<T> OperationApplyResult<T> {
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

    pub fn no_op(revision: u64) -> Self {
        Self {
            status: OperationApplyStatus::NoOp,
            inverse: None,
            affected_ids: Vec::new(),
            before_revision: revision,
            after_revision: revision,
        }
    }

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

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationFailure {
    pub transaction_id: u64,
    pub target_id: OperationTargetId,
    pub kind: String,
    pub version: u32,
    pub origin: OperationOrigin,
    pub error_code: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationEvent {
    Committed(OperationReceipt),
    Failed(OperationFailure),
}
