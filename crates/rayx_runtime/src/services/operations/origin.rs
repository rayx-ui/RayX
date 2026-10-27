#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationOrigin {
    LocalUser,
    System,
    Import,
    HistoryUndo,
    HistoryRedo,
    Remote(String),
}

impl OperationOrigin {
    pub fn is_local_user(&self) -> bool {
        matches!(self, Self::LocalUser)
    }
}
