//! The history's options for TSX (`addHistory`, `addPersistentHistory`).

use nsh_core::prelude::*;
use rayx_app_nsh::ts::push;

use crate::timer::HistoryOptions as History;

/// RayX's history's options (`addHistory`, `addPersistentHistory`), as Rust's `HistoryOptions`
/// sets them: each method sets one and returns the object, so calls chain.
#[class]
pub struct HistoryOptions {
    steps: Vec<HistoryStep>,
}

/// What one `HistoryOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum HistoryStep {
    CallTimeoutMs(Option<u64>),
}

impl HistoryStep {
    pub(crate) fn apply(&self, options: &mut History) {
        match self {
            Self::CallTimeoutMs(timeout) => options.call_timeout_ms = *timeout,
        }
    }
}

impl HistoryOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<HistoryStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl HistoryOptions {
    /// How long a call of the operation service or the store may take, in milliseconds, before
    /// the command making it fails (timed out) and frees its scope; `null` for no limit (30
    /// seconds by default).
    pub fn call_timeout_ms(this: Instance<Self>, timeout: Option<u32>) -> Instance<Self> {
        push(this, |o| {
            o.steps
                .push(HistoryStep::CallTimeoutMs(timeout.map(u64::from)))
        })
    }
}
