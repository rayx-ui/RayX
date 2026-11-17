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
            // `shift-cmd-p` binds what `cmd-shift-p` does.
            let key = (&entry.context, entry.binding.normalized());
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
    normalize_key_sequence(sequence).is_some()
}

/// `sequence` in one spelling, so that sequences that mean the same keys are equal: each
/// keystroke's modifiers in the order `ctrl`, `alt`, `shift`, `cmd`, `fn`, each once, and its
/// key in lowercase. `control` is `ctrl`, and `cmd`, `super`, `win` and `platform` are the
/// platform's key (Command on Apple platforms, the Windows or Super key elsewhere), as GPUI
/// reads them. `None` when it is no key sequence ([`valid_key_sequence`]).
///
/// `cmd-shift-P` and `shift-cmd-p` are both `shift-cmd-p`, `control-k control-c` is
/// `ctrl-k ctrl-c`, and the minus key alone (`-`, or `--`) is `-`.
pub fn normalize_key_sequence(sequence: &str) -> Option<String> {
    let strokes = sequence
        .split(' ')
        .map(normalize_keystroke)
        .collect::<Option<Vec<_>>>()?;
    (!strokes.is_empty()).then(|| strokes.join(" "))
}

fn normalize_keystroke(stroke: &str) -> Option<String> {
    // The modifiers' order, and their spellings.
    const ORDER: [&str; 5] = ["ctrl", "alt", "shift", "cmd", "fn"];
    let (modifiers, key) = if stroke == "-" {
        ("", "-")
    } else {
        match stroke.strip_suffix("--") {
            Some(rest) => (rest, "-"),
            None => match stroke.rsplit_once('-') {
                Some((modifiers, key)) => (modifiers, key),
                None => ("", stroke),
            },
        }
    };
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        return None;
    }
    let mut held = [false; ORDER.len()];
    if !modifiers.is_empty() {
        for modifier in modifiers.split('-') {
            let canonical = match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => "ctrl",
                "alt" => "alt",
                "shift" => "shift",
                "cmd" | "platform" | "super" | "win" => "cmd",
                "fn" => "fn",
                _ => return None,
            };
            let index = ORDER.iter().position(|name| *name == canonical)?;
            held[index] = true;
        }
    }
    let mut normalized: Vec<&str> = ORDER
        .iter()
        .zip(held)
        .filter_map(|(name, held)| held.then_some(*name))
        .collect();
    let key = key.to_lowercase();
    normalized.push(&key);
    Some(normalized.join("-"))
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

impl InputBinding {
    /// The binding in one spelling: a key sequence normalized ([`normalize_key_sequence`]),
    /// as it is when it is no key sequence; a pointer gesture as it is.
    pub fn normalized(&self) -> InputBinding {
        match self {
            Self::Keyboard(sequence) => {
                Self::Keyboard(normalize_key_sequence(sequence).unwrap_or_else(|| sequence.clone()))
            }
            Self::Pointer(gesture) => Self::Pointer(gesture.clone()),
        }
    }

    /// Whether `other` binds the same keys or gesture, however each spells it.
    pub fn same_as(&self, other: &InputBinding) -> bool {
        self.normalized() == other.normalized()
    }
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
