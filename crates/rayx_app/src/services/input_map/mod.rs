//! Input maps: which action a key sequence or a pointer gesture runs in a context, in three
//! layers (framework, application, user), published by an [`InputMapService`] implementation as
//! [`InputMapState`]. Resolving a binding ([`InputMapState::resolve`]) and listing an action's
//! shortcuts ([`InputMapState::bindings_for`]) are functions of the state, so they are
//! synchronous for any implementation; the UI layer feeds them the keystrokes and gestures it
//! sees.
//!
//! RayX's implementation is `rayx_services_input_map`'s (`add_input_map(..)`), which keeps the
//! user's document in the app's `dyn KeyValueStore`; an app may register its own, in Rust or in
//! TS.

mod model;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use model::{
    INPUT_MAP_SCHEMA_VERSION, InputBinding, InputMapDocument, InputMapEntry, InputMapError,
    InputMapSource, InputModifiers, InputPointerButton, InputPointerGesture, InputWheelDirection,
    InputWheelSource, InputWhen, normalize_key_sequence, resolve_key_sequence_for_platform,
    to_nsh_keystroke, valid_key_sequence,
};

use super::ContractFailure;
use super::storage::StorageError;
use crate::published::StateCell;

/// What a binding resolved to: the action to run, with its arguments and dispatch policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedInputAction {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// The context of the entry that matched.
    pub context: String,
    /// The layer the entry came from.
    pub source: InputMapSource,
    /// When the binding fires with respect to text editing focus.
    #[serde(default, skip_serializing_if = "InputWhen::is_auto")]
    pub when: InputWhen,
    /// Whether key auto-repeat fires the binding again.
    #[serde(default, skip_serializing_if = "model::is_false")]
    pub repeat: bool,
    /// Whether the key event passes through to the focused view after matching.
    #[serde(default, skip_serializing_if = "model::is_false")]
    pub pass_through: bool,
}

/// A winning effective binding in a context stack, ready to compile into a host keymap table.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedInputEntry {
    /// The normalized binding.
    pub binding: InputBinding,
    /// The action to run.
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// The context of the entry that won.
    pub context: String,
    /// The layer the entry came from.
    pub source: InputMapSource,
    /// When the binding fires with respect to text editing focus.
    #[serde(default, skip_serializing_if = "InputWhen::is_auto")]
    pub when: InputWhen,
    /// Whether key auto-repeat fires the binding again.
    #[serde(default, skip_serializing_if = "model::is_false")]
    pub repeat: bool,
    /// Whether the key event passes through to the focused view after matching.
    #[serde(default, skip_serializing_if = "model::is_false")]
    pub pass_through: bool,
}

/// Why the user's document was not loaded or applied.
#[derive(Debug, thiserror::Error)]
pub enum InputMapApplyError {
    #[error("the user's input map is at revision {actual}, not {expected}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error(transparent)]
    Model(#[from] InputMapError),
    #[error("the user's input map did not serialize: {0}")]
    Serialize(String),
    #[error("the user's input map was not stored: {0}")]
    Storage(#[from] StorageError),
    /// The call failed (a TS implementation threw, the engine is gone).
    #[error("{0}")]
    Failed(ContractFailure),
}

impl From<ContractFailure> for InputMapApplyError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The input map now: the framework's and the application's documents (from code and
/// configuration, never stored) and the user's, which goes over them, with the user's document's
/// revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputMapState {
    /// The bindings a UI library brings.
    pub framework: InputMapDocument,
    /// The app's own bindings, over the framework's.
    pub application: InputMapDocument,
    /// The user's bindings, over the other two; an entry with `unbind` removes the binding the
    /// layers below give its context.
    pub user: InputMapDocument,
    /// The user's document's revision (`user.revision`): what
    /// [`InputMapService::apply_user_document`] expects.
    pub revision: u64,
}

impl Default for InputMapState {
    fn default() -> Self {
        Self {
            framework: InputMapDocument::new(InputMapSource::Framework, 0, Vec::new()),
            application: InputMapDocument::new(InputMapSource::Application, 0, Vec::new()),
            user: InputMapDocument::new(InputMapSource::User, 0, Vec::new()),
            revision: 0,
        }
    }
}

impl InputMapState {
    /// The layers, highest first: the user's, the application's, the framework's.
    fn layers(&self) -> [(InputMapSource, &InputMapDocument); 3] {
        [
            (InputMapSource::User, &self.user),
            (InputMapSource::Application, &self.application),
            (InputMapSource::Framework, &self.framework),
        ]
    }

    /// The action `binding` runs in the innermost of `contexts` (innermost first) that binds
    /// it, the user's layer over the application's over the framework's; `""` (everywhere) is
    /// tried last. A key sequence matches whatever its spelling.
    pub fn resolve(
        &self,
        contexts: &[&str],
        binding: &InputBinding,
    ) -> Option<ResolvedInputAction> {
        let binding = binding.normalized();
        let global = [""];
        for context in contexts
            .iter()
            .chain(global.iter().filter(|_| !contexts.contains(&"")))
        {
            for (source, document) in self.layers() {
                if let Some(entry) = document.entries.iter().find(|entry| {
                    entry.context == *context && entry.binding.normalized() == binding
                }) {
                    if entry.unbind {
                        return None;
                    }
                    return Some(ResolvedInputAction {
                        action: entry.action.clone(),
                        arguments: entry.arguments.clone(),
                        context: entry.context.clone(),
                        source,
                        when: entry.when,
                        repeat: entry.repeat,
                        pass_through: entry.pass_through,
                    });
                }
            }
        }
        None
    }

    /// Like [`InputMapState::resolve`], but also resolves `mod` for the target platform
    /// (`apple == true` maps `mod` to `cmd`, `apple == false` maps `mod` to `ctrl`), so a user
    /// pressing `ctrl-s` on Windows/Linux or `cmd-s` on macOS matches a `mod-s` binding.
    pub fn resolve_for_platform(
        &self,
        contexts: &[&str],
        binding: &InputBinding,
        apple: bool,
    ) -> Option<ResolvedInputAction> {
        let normalized = binding.normalized();
        let resolved = binding.resolved_for_platform(apple);
        let global = [""];
        for context in contexts
            .iter()
            .chain(global.iter().filter(|_| !contexts.contains(&"")))
        {
            for (source, document) in self.layers() {
                let matched = document
                    .entries
                    .iter()
                    .find(|entry| {
                        entry.context == *context && entry.binding.normalized() == normalized
                    })
                    .or_else(|| {
                        document.entries.iter().find(|entry| {
                            entry.context == *context
                                && entry.binding.resolved_for_platform(apple) == resolved
                        })
                    });
                if let Some(entry) = matched {
                    if entry.unbind {
                        return None;
                    }
                    return Some(ResolvedInputAction {
                        action: entry.action.clone(),
                        arguments: entry.arguments.clone(),
                        context: entry.context.clone(),
                        source,
                        when: entry.when,
                        repeat: entry.repeat,
                        pass_through: entry.pass_through,
                    });
                }
            }
        }
        None
    }

    /// The winning effective entries across `contexts` (innermost first, plus `""`), with User
    /// over Application over Framework precedence and `unbind` entries respected.
    pub fn effective_entries_for_contexts(&self, contexts: &[&str]) -> Vec<ResolvedInputEntry> {
        let mut ordered_contexts: Vec<&str> = Vec::with_capacity(contexts.len() + 1);
        for &context in contexts {
            if !ordered_contexts.contains(&context) {
                ordered_contexts.push(context);
            }
        }
        if !ordered_contexts.contains(&"") {
            ordered_contexts.push("");
        }

        let mut seen = BTreeSet::<InputBinding>::new();
        let mut effective = Vec::new();
        for context in ordered_contexts {
            for (source, document) in self.layers() {
                for entry in &document.entries {
                    if entry.context != context {
                        continue;
                    }
                    let normalized = entry.binding.normalized();
                    if !seen.insert(normalized.clone()) {
                        continue;
                    }
                    if !entry.unbind {
                        effective.push(ResolvedInputEntry {
                            binding: normalized,
                            action: entry.action.clone(),
                            arguments: entry.arguments.clone(),
                            context: entry.context.clone(),
                            source,
                            when: entry.when,
                            repeat: entry.repeat,
                            pass_through: entry.pass_through,
                        });
                    }
                }
            }
        }
        effective
    }

    /// The bindings that run `action` (in any context), for showing shortcuts: each layer's,
    /// minus those a higher layer rebinds or unbinds in the same context.
    pub fn bindings_for(&self, action: &str) -> Vec<InputMapEntry> {
        let mut seen = Vec::<(&str, InputBinding)>::new();
        let mut found = Vec::new();
        for (_, document) in self.layers() {
            for entry in &document.entries {
                let key = (entry.context.as_str(), entry.binding.normalized());
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                if !entry.unbind && entry.action == action {
                    found.push(entry.clone());
                }
            }
        }
        found
    }
}

/// The input map service: the three layers of bindings, published as [`InputMapState`], and the
/// commands that read and replace the user's.
#[rayx_app_macros::contract(ts = "InputMapServiceContract")]
pub trait InputMapService {
    /// The documents and the user's revision now, and each change of them.
    #[state(ts = "InputMapState")]
    fn input_map(&self) -> &StateCell<InputMapState>;

    /// Reads the user's document again from where the implementation keeps it (RayX's: the
    /// app's key-value store, which it reads at startup) and makes it current.
    async fn load(&self) -> Result<InputMapDocument, InputMapApplyError>;

    /// Replaces the user's document with `candidate` when the current one is at
    /// `expected_revision`: the candidate gets the next revision, is validated and kept, and
    /// only then becomes current. A failure leaves the current document as it was.
    #[ts("(expectedRevision: number, candidate: InputMapDocument): InputMapDocument")]
    async fn apply_user_document(
        &self,
        expected_revision: u64,
        candidate: InputMapDocument,
    ) -> Result<InputMapDocument, InputMapApplyError>;
}

impl dyn InputMapService {
    /// The action `binding` runs in the innermost of `contexts` ([`InputMapState::resolve`]).
    pub fn resolve(
        &self,
        contexts: &[&str],
        binding: &InputBinding,
    ) -> Option<ResolvedInputAction> {
        self.input_map().get().resolve(contexts, binding)
    }

    /// The action `binding` runs in the innermost of `contexts` for the given platform
    /// ([`InputMapState::resolve_for_platform`]).
    pub fn resolve_for_platform(
        &self,
        contexts: &[&str],
        binding: &InputBinding,
        apple: bool,
    ) -> Option<ResolvedInputAction> {
        self.input_map()
            .get()
            .resolve_for_platform(contexts, binding, apple)
    }

    /// The winning effective entries across `contexts`
    /// ([`InputMapState::effective_entries_for_contexts`]).
    pub fn effective_entries_for_contexts(&self, contexts: &[&str]) -> Vec<ResolvedInputEntry> {
        self.input_map()
            .get()
            .effective_entries_for_contexts(contexts)
    }

    /// The bindings that run `action` ([`InputMapState::bindings_for`]).
    pub fn bindings_for(&self, action: &str) -> Vec<InputMapEntry> {
        self.input_map().get().bindings_for(action)
    }

    /// The user's document.
    pub fn user_document(&self) -> InputMapDocument {
        self.input_map().get().user.clone()
    }

    /// The user's document's revision.
    pub fn revision(&self) -> u64 {
        self.input_map().get().revision
    }

    /// The framework's document.
    pub fn framework_document(&self) -> InputMapDocument {
        self.input_map().get().framework.clone()
    }

    /// The application's document.
    pub fn application_document(&self) -> InputMapDocument {
        self.input_map().get().application.clone()
    }
}
