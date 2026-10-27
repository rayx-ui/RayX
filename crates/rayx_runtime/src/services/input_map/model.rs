use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The version of [`InputMapDocument`]'s shape.
pub const INPUT_MAP_SCHEMA_VERSION: u32 = 1;

/// The only persisted input-map document. Built-in and application maps use the
/// same shape but are supplied by the application rather than written to storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputMapDocument {
    pub schema_version: u32,
    pub source: InputMapSource,
    pub revision: u64,
    #[serde(default)]
    pub entries: Vec<InputMapEntry>,
}

impl InputMapDocument {
    pub fn new(source: InputMapSource, revision: u64, entries: Vec<InputMapEntry>) -> Self {
        Self {
            schema_version: INPUT_MAP_SCHEMA_VERSION,
            source,
            revision,
            entries,
        }
    }

    pub fn validate(&self) -> Result<(), InputMapError> {
        if self.schema_version != INPUT_MAP_SCHEMA_VERSION {
            return Err(InputMapError::UnsupportedSchema(self.schema_version));
        }

        let mut bindings = BTreeSet::new();
        for entry in &self.entries {
            if entry.action.trim().is_empty() || entry.context.trim() != entry.context {
                return Err(InputMapError::InvalidEntry);
            }
            if let InputBinding::Keyboard(sequence) = &entry.binding
                && !valid_key_sequence(sequence)
            {
                return Err(InputMapError::InvalidEntry);
            }
            if let InputBinding::Pointer(gesture) = &entry.binding
                && !valid_pointer_gesture(gesture)
            {
                return Err(InputMapError::InvalidEntry);
            }
            let key = (&entry.context, &entry.binding);
            if !bindings.insert(key) {
                return Err(InputMapError::DuplicateBinding);
            }
        }
        Ok(())
    }
}

/// Whether `sequence` is a key sequence: keystrokes separated by spaces, each modifiers and a
/// key joined by `-` (`ctrl-k ctrl-c`, `cmd-shift-p`, `ctrl--` for the minus key).
pub fn valid_key_sequence(sequence: &str) -> bool {
    const MODIFIERS: [&str; 9] = [
        "ctrl", "control", "alt", "shift", "cmd", "platform", "super", "win", "fn",
    ];
    let strokes: Vec<&str> = sequence.split(' ').collect();
    !strokes.is_empty()
        && strokes.iter().all(|stroke| {
            let (modifiers, key) = match stroke.strip_suffix("--") {
                Some(rest) => (rest, "-"),
                None => match stroke.rsplit_once('-') {
                    Some((modifiers, key)) => (modifiers, key),
                    None => ("", *stroke),
                },
            };
            !key.is_empty()
                && !key.chars().any(char::is_whitespace)
                && (modifiers.is_empty()
                    || modifiers.split('-').all(|modifier| {
                        MODIFIERS.contains(&modifier.to_ascii_lowercase().as_str())
                    }))
        })
}

pub(crate) fn valid_pointer_gesture(gesture: &InputPointerGesture) -> bool {
    match gesture {
        InputPointerGesture::Click { count, .. } => *count > 0,
        InputPointerGesture::Drag { threshold, .. }
        | InputPointerGesture::Wheel { threshold, .. } => *threshold > 0,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMapSource {
    Framework,
    Application,
    User,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputMapEntry {
    pub context: String,
    pub binding: InputBinding,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(default)]
    pub unbind: bool,
}

impl InputMapEntry {
    pub fn keyboard(
        context: impl Into<String>,
        sequence: impl Into<String>,
        action: impl Into<String>,
        arguments: Option<serde_json::Value>,
    ) -> Self {
        Self {
            context: context.into(),
            binding: InputBinding::Keyboard(sequence.into()),
            action: action.into(),
            arguments,
            unbind: false,
        }
    }

    pub fn pointer(
        context: impl Into<String>,
        gesture: InputPointerGesture,
        action: impl Into<String>,
        arguments: Option<serde_json::Value>,
    ) -> Self {
        Self {
            context: context.into(),
            binding: InputBinding::Pointer(gesture),
            action: action.into(),
            arguments,
            unbind: false,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum InputBinding {
    Keyboard(String),
    Pointer(InputPointerGesture),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputPointerGesture {
    Click {
        button: InputPointerButton,
        count: u8,
        modifiers: InputModifiers,
    },
    Drag {
        button: InputPointerButton,
        threshold: u16,
        modifiers: InputModifiers,
    },
    Wheel {
        direction: InputWheelDirection,
        source: InputWheelSource,
        threshold: u16,
        modifiers: InputModifiers,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputPointerButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
    Other(u8),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputWheelDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputWheelSource {
    Mouse,
    Touchpad,
    PrecisionTouchpad,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputModifiers {
    #[serde(default)]
    pub control: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub platform: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputMapError {
    UnsupportedSchema(u32),
    InvalidEntry,
    DuplicateBinding,
    UserDocumentRequired,
    InvalidDocumentSource,
    /// A stored document did not read back.
    Corrupt(String),
}

impl std::fmt::Display for InputMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported input map schema version {version}")
            }
            Self::InvalidEntry => formatter.write_str("input map entry is invalid"),
            Self::DuplicateBinding => formatter.write_str("input map contains a duplicate binding"),
            Self::UserDocumentRequired => {
                formatter.write_str("a user input map document is required")
            }
            Self::InvalidDocumentSource => {
                formatter.write_str("input map document has an invalid source")
            }
            Self::Corrupt(reason) => write!(formatter, "the stored input map is corrupt: {reason}"),
        }
    }
}

impl std::error::Error for InputMapError {}
