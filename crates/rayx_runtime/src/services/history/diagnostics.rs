#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryDiagnostics {
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub stack_revision: u64,
    pub subscriber_count: usize,
    pub unavailable_entry_count: usize,
    pub recent_failure_codes: Vec<String>,
}
