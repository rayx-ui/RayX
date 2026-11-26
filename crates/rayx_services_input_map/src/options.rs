use serde::{Deserialize, Serialize};

use rayx_app::services::input_map::{InputMapDocument, InputMapEntry, InputMapSource};

/// The configuration section [`InputMapOptions`] bind from.
pub const INPUT_MAP_OPTIONS_SECTION: &str = "RayX:Services:InputMap";

/// RayX's input map's options: the framework's and the application's bindings (the user's are
/// stored, not configured), and an optional asset path whose JSON supplies the application's
/// default bindings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputMapOptions {
    /// The bindings a UI library brings.
    pub framework: Vec<InputMapEntry>,
    /// The app's own bindings, over the framework's.
    pub application: Vec<InputMapEntry>,
    /// Optional asset path (resolved through the app's `dyn AssetService`, e.g.
    /// `"keybindings.json"`) whose JSON supplies the application's default bindings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_asset: Option<String>,
}

impl InputMapOptions {
    /// Sets the asset path (`"keybindings.json"`, `"@my-app/keybindings.json"`) whose JSON
    /// supplies the application's default bindings, loaded from the app's `dyn AssetService`
    /// when the input map starts.
    pub fn application_asset(&mut self, path: impl Into<String>) -> &mut Self {
        self.application_asset = Some(path.into());
        self
    }

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

    /// Adds an application binding of the key sequence `sequence` (`mod-s`, `ctrl-k ctrl-c`) to
    /// `action` in `context`, with additional dispatch policy or arguments configured by
    /// `configure` (`with_when`, `with_repeat`, `with_pass_through`).
    pub fn bind_key_with(
        &mut self,
        context: impl Into<String>,
        sequence: impl Into<String>,
        action: impl Into<String>,
        configure: impl FnOnce(InputMapEntry) -> InputMapEntry,
    ) -> &mut Self {
        let entry = configure(InputMapEntry::keyboard(context, sequence, action, None));
        self.application.push(entry);
        self
    }

    pub(crate) fn is_valid(&self) -> bool {
        if let Some(path) = &self.application_asset
            && path.trim().is_empty()
        {
            return false;
        }
        let (framework, application) = self.documents();
        framework.validate().is_ok() && application.validate().is_ok()
    }

    pub(crate) fn documents(&self) -> (InputMapDocument, InputMapDocument) {
        (
            InputMapDocument::new(InputMapSource::Framework, 0, self.framework.clone()),
            InputMapDocument::new(InputMapSource::Application, 0, self.application.clone()),
        )
    }
}
