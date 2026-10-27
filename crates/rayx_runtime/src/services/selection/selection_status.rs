use serde::{Deserialize, Serialize};

/// Content-free application summary for the active selection area.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionStatus {
    pub participant_count: usize,
    pub selected_character_count: usize,
    pub is_collapsed: bool,
    pub is_selecting: bool,
}

impl SelectionStatus {
    pub const fn has_selection(self) -> bool {
        self.selected_character_count > 0 || self.is_collapsed
    }
}
