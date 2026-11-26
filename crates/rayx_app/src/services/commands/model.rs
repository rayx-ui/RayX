use serde::{Deserialize, Serialize};

/// The active contexts now, innermost first; global (`""`) is always implicit last.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextState {
    /// Active context names, innermost first (deduplicated, never containing `""`).
    #[serde(default)]
    pub contexts: Vec<String>,
    /// Increases whenever the active context list changes.
    #[serde(default)]
    pub revision: u64,
}

impl ContextState {
    /// Active context names, innermost first.
    pub fn active_contexts(&self) -> &[String] {
        &self.contexts
    }

    /// Whether `context` is currently active (`""` is always active as the global context).
    pub fn has_context(&self, context: &str) -> bool {
        context.is_empty() || self.contexts.iter().any(|active| active == context)
    }
}

/// Public metadata describing a registered command and its current state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandMetadata {
    /// Unique command identifier (e.g. `"edit.undo"`, `"file.save"`).
    pub id: String,
    /// Human-readable title shown in menus, toolbars, and command palettes.
    pub title: String,
    /// Optional grouping category (e.g. `"Edit"`, `"View"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Optional classification kind (e.g. `"action"`, `"mutation"`, `"intent"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Non-empty contexts declared for this command or its contextual handlers (empty means global).
    #[serde(default)]
    pub contexts: Vec<String>,
    /// Whether the command can currently execute in the active contexts.
    pub can_execute: bool,
    /// Whether an execution of this command is currently in flight.
    pub is_executing: bool,
    /// Optional toggle/checked state for checkable commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

/// The registered commands and their current metadata, published by a [`super::CommandService`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandState {
    /// Registered commands, sorted by `id`.
    #[serde(default)]
    pub commands: Vec<CommandMetadata>,
    /// Increases whenever the command list or any command's metadata changes.
    #[serde(default)]
    pub revision: u64,
}

impl CommandState {
    /// Metadata for command `id`, if registered.
    pub fn get(&self, id: &str) -> Option<&CommandMetadata> {
        self.commands.iter().find(|command| command.id == id)
    }

    /// Whether a command with `id` is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Whether command `id` is registered and currently reports `can_execute`.
    pub fn can_execute(&self, id: &str) -> bool {
        self.get(id).is_some_and(|command| command.can_execute)
    }

    /// Whether command `id` is currently executing.
    pub fn is_executing(&self, id: &str) -> bool {
        self.get(id).is_some_and(|command| command.is_executing)
    }
}

/// Events emitted by a [`super::CommandService`] as commands are registered, unregistered,
/// executed, or fail.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CommandEvent {
    /// A command or contextual handler was registered for this identifier.
    Registered(String),
    /// A command or contextual handler was unregistered for this identifier.
    Unregistered(String),
    /// A command completed execution successfully.
    Executed { id: String },
    /// A command failed during execution.
    Failed { id: String, message: String },
}

/// An active keyboard binding resolved from the current contexts and input map, ready to be
/// mounted on a keymap surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActiveKeyBinding {
    /// Single keystroke in `+`-delimited syntax (e.g. `"mod+z"`, `"ctrl+shift+p"`, `"escape"`).
    pub keystroke: String,
    /// Command or action identifier invoked when the keystroke fires.
    pub action: String,
    /// Optional JSON arguments passed when executing the action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// The context the winning entry matched (`""` for global).
    pub context: String,
    /// When the binding fires (`"always"`, `"auto"`, `"notEditing"`, or omitted for default auto).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Whether auto-repeats of a held key fire the binding again.
    #[serde(default)]
    pub repeat: bool,
    /// Whether the key event continues propagating after the binding fires.
    #[serde(default, rename = "passThrough")]
    pub pass_through: bool,
}
