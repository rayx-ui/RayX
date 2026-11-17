use serde::{Deserialize, Serialize};

/// The content-free summary of the active area's selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionStatus {
    /// How many participants (text runs, cells) the selection spans.
    pub participant_count: usize,
    /// How many characters are selected.
    pub selected_character_count: usize,
    /// Whether the selection is a caret (empty, collapsed).
    pub is_collapsed: bool,
    /// Whether a selection is in progress (a drag).
    pub is_selecting: bool,
}

impl SelectionStatus {
    /// Whether anything is selected, or a caret is placed.
    pub const fn has_selection(self) -> bool {
        self.selected_character_count > 0 || self.is_collapsed
    }
}
