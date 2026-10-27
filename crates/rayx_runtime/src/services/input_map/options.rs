use serde::{Deserialize, Serialize};

use super::{InputMapDocument, InputMapEntry, InputMapSource};

/// The configuration section [`InputMapOptions`] bind from.
pub const INPUT_MAP_OPTIONS_SECTION: &str = "RayX:Services:InputMap";

/// The framework's and the application's bindings (the user's are stored, not configured).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputMapOptions {
    /// The bindings a UI library brings.
    pub framework: Vec<InputMapEntry>,
    /// The app's own bindings, over the framework's.
    pub application: Vec<InputMapEntry>,
}

impl InputMapOptions {
    /// Adds an application binding of the key sequence `sequence` (`ctrl-k ctrl-c`) to
    /// `action` in `context`.
    pub fn bind_key(
        &mut self,
        context: impl Into<String>,
        sequence: impl Into<String>,
        action: impl Into<String>,
    ) -> &mut Self {
        self.application
            .push(InputMapEntry::keyboard(context, sequence, action, None));
        self
    }

    pub(super) fn documents(&self) -> (InputMapDocument, InputMapDocument) {
        (
            InputMapDocument::new(InputMapSource::Framework, 0, self.framework.clone()),
            InputMapDocument::new(InputMapSource::Application, 0, self.application.clone()),
        )
    }
}
