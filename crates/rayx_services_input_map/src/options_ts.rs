//! The input map's options for TSX (`addInputMap`).

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::input_map::{InputMapEntry, InputWhen};
use rayx_app_nsh::ts::push;
use serde::Deserialize;

use crate::options::InputMapOptions as InputMap;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyBindingOptions {
    #[serde(default)]
    arguments: Option<serde_json::Value>,
    #[serde(default)]
    when: InputWhen,
    #[serde(default)]
    repeat: bool,
    #[serde(default)]
    pass_through: bool,
}

/// The input map's options (`addInputMap`), as Rust's `InputMapOptions` sets them: `bindKey`
/// and `bindKeyWith` add bindings and return the object, so calls chain.
#[class]
pub struct InputMapOptions {
    entries: Vec<InputMapEntry>,
}

impl InputMapOptions {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn take(&mut self) -> Vec<InputMapEntry> {
        std::mem::take(&mut self.entries)
    }

    pub(crate) fn apply(entry: &InputMapEntry, options: &mut InputMap) {
        options.application.push(entry.clone());
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
        push(this, |o| {
            o.entries
                .push(InputMapEntry::keyboard(context, sequence, action, None));
        })
    }

    /// Adds an application binding of the key sequence `sequence` (`"mod-s"`) to `action` in
    /// `context`, with optional dispatch policy (`when`, `repeat`, `pass_through`) and
    /// `arguments`.
    #[ts(
        "(context: string, sequence: string, action: string, options?: InputKeyBindingOptions | null): InputMapOptions"
    )]
    pub fn bind_key_with(
        this: Instance<Self>,
        context: String,
        sequence: String,
        action: String,
        options: Option<JsonValue>,
    ) -> Result<Instance<Self>, String> {
        let parsed: KeyBindingOptions = match options {
            Some(JsonValue::Null) | None => KeyBindingOptions::default(),
            Some(value) => {
                serde_json::from_value(value).map_err(|error| format!("options: {error}"))?
            }
        };
        let entry = InputMapEntry::keyboard(context, sequence, action, parsed.arguments)
            .with_when(parsed.when)
            .with_repeat(parsed.repeat)
            .with_pass_through(parsed.pass_through);
        Ok(push(this, |o| o.entries.push(entry)))
    }
}
