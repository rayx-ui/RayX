use super::{HistoryEntry, HistoryNavigationFailure, HistoryScopeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryEvent {
    Recorded(HistoryEntry),
    ScopeCleared(HistoryScopeId),
    AvailabilityChanged(HistoryScopeId),
    UndoCompleted(HistoryScopeId),
    RedoCompleted(HistoryScopeId),
    NavigationFailed(HistoryNavigationFailure),
}
