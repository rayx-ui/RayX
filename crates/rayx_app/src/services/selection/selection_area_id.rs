use serde::{Deserialize, Serialize};

/// The identity of one visual selection area in the app's summary (a number in TS).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SelectionAreaId(u64);

impl SelectionAreaId {
    /// The area numbered `value`.
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Its number.
    pub const fn get(self) -> u64 {
        self.0
    }
}
