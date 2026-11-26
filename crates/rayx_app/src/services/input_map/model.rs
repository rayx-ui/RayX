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
/// key joined by `-` (`ctrl-k ctrl-c`, `mod-shift-p`, `ctrl--` for the minus key).
pub fn valid_key_sequence(sequence: &str) -> bool {
    normalize_key_sequence(sequence).is_some()
}

/// Canonical modifier order in normalized key sequences.
const MODIFIER_ORDER: [&str; 6] = ["mod", "ctrl", "alt", "shift", "cmd", "fn"];
const MOD_IDX: usize = 0;
const CTRL_IDX: usize = 1;
const ALT_IDX: usize = 2;
const SHIFT_IDX: usize = 3;
const CMD_IDX: usize = 4;
const FN_IDX: usize = 5;

struct ParsedKeystroke {
    modifiers: [bool; MODIFIER_ORDER.len()],
    key: String,
}

impl ParsedKeystroke {
    fn parse(stroke: &str) -> Option<Self> {
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
        let mut held = [false; MODIFIER_ORDER.len()];
        if !modifiers.is_empty() {
            for modifier in modifiers.split('-') {
                let index = match modifier.to_ascii_lowercase().as_str() {
                    "mod" | "cmdorctrl" | "commandorcontrol" | "platform" => MOD_IDX,
                    "ctrl" | "control" => CTRL_IDX,
                    "alt" | "option" | "opt" => ALT_IDX,
                    "shift" => SHIFT_IDX,
                    "cmd" | "command" | "meta" | "super" | "win" => CMD_IDX,
                    "fn" => FN_IDX,
                    _ => return None,
                };
                held[index] = true;
            }
        }
        Some(Self {
            modifiers: held,
            key: key.to_lowercase(),
        })
    }

    fn format(&self) -> String {
        let mut normalized: Vec<&str> = MODIFIER_ORDER
            .iter()
            .zip(self.modifiers)
            .filter_map(|(name, held)| held.then_some(*name))
            .collect();
        normalized.push(&self.key);
        normalized.join("-")
    }
}

/// `sequence` in one spelling, so that sequences that mean the same keys are equal: each
/// keystroke's modifiers in the order `mod`, `ctrl`, `alt`, `shift`, `cmd`, `fn`, each once,
/// and its key in lowercase. `mod`, `cmdorctrl`, `commandorcontrol`, and `platform` normalize
/// to `mod` (the OS-aware primary shortcut modifier: Command on Apple platforms, Control
/// elsewhere) and stay distinct from literal `cmd` (`command`, `meta`, `super`, `win`).
/// `control` is `ctrl`, and `option` and `opt` are `alt`. `None` when it is no key sequence
/// ([`valid_key_sequence`]).
///
/// `mod-shift-P` and `shift-platform-p` are both `mod-shift-p`, `cmd-shift-P` and
/// `shift-cmd-p` are both `shift-cmd-p`, `control-k control-c` is `ctrl-k ctrl-c`, and the
/// minus key alone (`-`, or `--`) is `-`.
pub fn normalize_key_sequence(sequence: &str) -> Option<String> {
    let strokes = sequence
        .split(' ')
        .map(normalize_keystroke)
        .collect::<Option<Vec<_>>>()?;
    (!strokes.is_empty()).then(|| strokes.join(" "))
}

fn normalize_keystroke(stroke: &str) -> Option<String> {
    ParsedKeystroke::parse(stroke).map(|parsed| parsed.format())
}

/// Resolves `mod` in `sequence` to `cmd` when `apple` is `true` or `ctrl` when `false`, then
/// re-normalizes (deduplicating modifiers, e.g. `mod-ctrl-a` with `apple = false` becomes
/// `ctrl-a`). Returns `None` when `sequence` is not a valid key sequence.
pub fn resolve_key_sequence_for_platform(sequence: &str, apple: bool) -> Option<String> {
    let strokes = sequence
        .split(' ')
        .map(|stroke| {
            let mut parsed = ParsedKeystroke::parse(stroke)?;
            if parsed.modifiers[MOD_IDX] {
                parsed.modifiers[MOD_IDX] = false;
                if apple {
                    parsed.modifiers[CMD_IDX] = true;
                } else {
                    parsed.modifiers[CTRL_IDX] = true;
                }
            }
            Some(parsed.format())
        })
        .collect::<Option<Vec<_>>>()?;
    (!strokes.is_empty()).then(|| strokes.join(" "))
}

/// Converts a single-stroke key sequence (no spaces) into NSH `keymap` `+`-separated syntax
/// (`"mod-shift-z"` -> `"mod+shift+z"`, `"mod-="` -> `"mod+plus"`, `"mod--"` -> `"mod+-"`,
/// `"mod-+"` -> `"mod+plus"`). Returns `None` for multi-stroke chord sequences, invalid
/// sequences, or keystrokes containing modifiers NSH's keymap does not support (`fn`).
pub fn to_nsh_keystroke(sequence: &str) -> Option<String> {
    if sequence.is_empty() || sequence.chars().any(char::is_whitespace) {
        return None;
    }
    let parsed = ParsedKeystroke::parse(sequence)?;
    if parsed.modifiers[FN_IDX] {
        return None;
    }
    let mut parts: Vec<&str> = MODIFIER_ORDER
        .iter()
        .zip(parsed.modifiers)
        .filter_map(|(name, held)| held.then_some(*name))
        .collect();
    let key = match parsed.key.as_str() {
        "=" | "+" => "plus",
        other => other,
    };
    parts.push(key);
    Some(parts.join("+"))
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

/// When a keyboard binding fires with respect to text editing focus.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputWhen {
    /// Let the host decide from the keystroke (keys that edit text stay in text inputs).
    #[default]
    Auto,
    /// Fire even while a text input has keyboard focus.
    Always,
    /// Only fire when no text input has keyboard focus.
    NotEditing,
}

impl InputWhen {
    /// Whether this policy is [`InputWhen::Auto`] (the default).
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
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
    #[serde(default, skip_serializing_if = "InputWhen::is_auto")]
    pub when: InputWhen,
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeat: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pass_through: bool,
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
            when: InputWhen::Auto,
            repeat: false,
            pass_through: false,
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
            when: InputWhen::Auto,
            repeat: false,
            pass_through: false,
        }
    }

    /// Sets when this binding fires with respect to text editing focus.
    pub fn with_when(mut self, when: InputWhen) -> Self {
        self.when = when;
        self
    }

    /// Sets whether key auto-repeat fires this binding again.
    pub fn with_repeat(mut self, repeat: bool) -> Self {
        self.repeat = repeat;
        self
    }

    /// Sets whether the key event passes through to the focused view after matching.
    pub fn with_pass_through(mut self, pass_through: bool) -> Self {
        self.pass_through = pass_through;
        self
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

    /// The binding resolved for the target platform (`mod` -> `cmd` when `apple` is `true`,
    /// `ctrl` when `false`, then re-normalized).
    pub fn resolved_for_platform(&self, apple: bool) -> InputBinding {
        match self {
            Self::Keyboard(sequence) => Self::Keyboard(
                resolve_key_sequence_for_platform(sequence, apple)
                    .unwrap_or_else(|| sequence.clone()),
            ),
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
