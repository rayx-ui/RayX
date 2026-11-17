//! The input map's options for TSX (`addInputMap`).

use nsh_core::prelude::*;
use rayx_runtime::services::input_map::InputMapOptions as InputMap;

use crate::configuration::options_ts::push;

/// The input map's options (`addInputMap`), as Rust's `InputMapOptions` sets them: `bindKey`
/// adds a binding and returns the object, so calls chain.
#[class]
pub struct InputMapOptions {
    bindings: Vec<(String, String, String)>,
}

impl InputMapOptions {
    pub(crate) fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub(crate) fn take(&mut self) -> Vec<(String, String, String)> {
        std::mem::take(&mut self.bindings)
    }

    pub(crate) fn apply(binding: &(String, String, String), options: &mut InputMap) {
        let (context, sequence, action) = binding.clone();
        options.bind_key(context, sequence, action);
    }
}

#[class]
impl InputMapOptions {
    /// Adds an application binding of the key sequence `sequence` (`"ctrl-k ctrl-c"`) to
    /// `action` in `context`.
    pub fn bind_key(
        this: Instance<Self>,
        context: String,
        sequence: String,
        action: String,
    ) -> Instance<Self> {
        push(this, |o| o.bindings.push((context, sequence, action)))
    }
}
