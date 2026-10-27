use crate::services::operations::{OperationRecord, OperationTargetId};
use serde::{Deserialize, Serialize};

use super::HistoryScopeId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryEntry {
    pub transaction_id: u64,
    pub scope_id: HistoryScopeId,
    pub target_id: OperationTargetId,
    pub label_key: String,
    pub forward: Vec<OperationRecord>,
    pub inverse: Vec<OperationRecord>,
    pub before_revision: u64,
    pub after_revision: u64,
}
