use super::{HistoryEntry, HistoryScopeId};
use serde::{Deserialize, Serialize};

pub const HISTORY_JOURNAL_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryJournalV2 {
    pub version: u32,
    pub revision: u64,
    pub scope_id: HistoryScopeId,
    pub entries: Vec<HistoryEntry>,
}

impl HistoryJournalV2 {
    pub fn new(revision: u64, scope_id: HistoryScopeId, entries: Vec<HistoryEntry>) -> Self {
        Self {
            version: HISTORY_JOURNAL_VERSION,
            revision,
            scope_id,
            entries,
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, HistoryJournalError> {
        serde_json::to_vec(self).map_err(|_| HistoryJournalError::Encode)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, HistoryJournalError> {
        let journal: Self =
            serde_json::from_slice(bytes).map_err(|_| HistoryJournalError::Corrupt)?;
        if journal.version != HISTORY_JOURNAL_VERSION {
            return Err(HistoryJournalError::UnsupportedVersion);
        }
        Ok(journal)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryJournalError {
    Corrupt,
    UnsupportedVersion,
    Encode,
}
