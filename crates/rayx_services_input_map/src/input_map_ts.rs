//! The input map service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::input_map::{
    InputBinding, InputMapDocument, InputMapService as InputMap, to_nsh_keystroke,
};
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

/// The app's input map, for TSX: `services.getRequiredService(InputMapService)`, what a binding
/// runs (`resolve(["Editor"], { kind: "keyboard", value: "ctrl-s" })`), an action's shortcuts
/// (`bindingsFor("save")`), the documents (`current()`) and the user's document, replaced with
/// `applyUserDocument` (a promise). `onChange` hears every change of the state.
///
/// It wraps whichever `dyn InputMapService` is registered (RayX's, `addInputMap`, or a TS
/// implementation of the contract `InputMapServiceContract` registered under this class).
#[class]
pub struct InputMapService {
    service: Arc<dyn InputMap>,
    listeners: Listeners,
}

impl ServiceClass for InputMapService {
    type Service = dyn InputMap;

    fn wrap(service: Arc<dyn InputMap>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

/// The service `this` wraps.
fn service(this: &Instance<InputMapService>) -> Arc<dyn InputMap> {
    this.with(|this| Arc::clone(&this.service))
}

#[class]
impl InputMapService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): InputMapServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The framework's, the application's and the user's documents and the user's revision now.
    #[ts("(): InputMapState")]
    pub fn current(&self) -> JsonValue {
        json(&*self.service.input_map().get())
    }

    /// The user's document's revision: what `applyUserDocument` expects.
    pub fn revision(&self) -> u64 {
        self.service.revision()
    }

    /// The action `binding` runs in the innermost of `contexts` (innermost first) that binds
    /// it, the user's layer over the application's over the framework's, `""` (everywhere)
    /// last; `null` when none does.
    #[ts("(contexts: string[], binding: InputBinding): ResolvedInputAction | null")]
    pub fn resolve(&self, contexts: Vec<String>, binding: JsonValue) -> Result<JsonValue, String> {
        let binding: InputBinding =
            serde_json::from_value(binding).map_err(|error| format!("binding: {error}"))?;
        let contexts: Vec<&str> = contexts.iter().map(String::as_str).collect();
        Ok(self
            .service
            .resolve(&contexts, &binding)
            .map_or(JsonValue::Null, |resolved| json(&resolved)))
    }

    /// Like `resolve`, resolving `mod` to `cmd` when `apple` is `true` or `ctrl` when `false`
    /// so platform-specific key presses match OS-aware `mod-...` bindings.
    #[ts("(contexts: string[], binding: InputBinding, apple: boolean): ResolvedInputAction | null")]
    pub fn resolve_for_platform(
        &self,
        contexts: Vec<String>,
        binding: JsonValue,
        apple: bool,
    ) -> Result<JsonValue, String> {
        let binding: InputBinding =
            serde_json::from_value(binding).map_err(|error| format!("binding: {error}"))?;
        let contexts: Vec<&str> = contexts.iter().map(String::as_str).collect();
        Ok(self
            .service
            .resolve_for_platform(&contexts, &binding, apple)
            .map_or(JsonValue::Null, |resolved| json(&resolved)))
    }

    /// The winning effective entries across `contexts` (innermost first, plus `""`), with the
    /// user's layer over the application's over the framework's and `unbind` entries respected.
    #[ts("(contexts: string[]): ResolvedInputEntry[]")]
    pub fn effective_entries(&self, contexts: Vec<String>) -> JsonValue {
        let contexts: Vec<&str> = contexts.iter().map(String::as_str).collect();
        json(&self.service.effective_entries_for_contexts(&contexts))
    }

    /// Converts a single-stroke key sequence (`"mod-shift-z"`, `"mod-="`, `"mod--"`) into NSH
    /// keymap `+`-separated syntax (`"mod+shift+z"`, `"mod+plus"`, `"mod+-"`), or `null` for
    /// multi-stroke chords or invalid sequences.
    #[ts("(sequence: string): string | null")]
    pub fn to_nsh_keystroke(&self, sequence: String) -> Option<String> {
        to_nsh_keystroke(&sequence)
    }

    /// The bindings that run `action` in any context, for showing shortcuts.
    #[ts("(action: string): InputMapEntry[]")]
    pub fn bindings_for(&self, action: String) -> JsonValue {
        json(&self.service.bindings_for(&action))
    }

    /// Reads the user's document again (RayX's service: from the app's key-value store);
    /// resolves with it, or rejects with the service's error.
    #[ts("(): InputMapDocument")]
    pub async fn load(this: Instance<Self>) -> Result<JsonValue, String> {
        service(&this)
            .load()
            .await
            .map(|document| json(&document))
            .map_err(|error| error.to_string())
    }

    /// Replaces the user's document with `candidate` when it is at `expectedRevision`; resolves
    /// with the document applied (at the next revision), or rejects with the service's error
    /// (a revision conflict, an invalid or duplicate binding), leaving the document as it was.
    #[ts("(expectedRevision: number, candidate: InputMapDocument): InputMapDocument")]
    pub async fn apply_user_document(
        this: Instance<Self>,
        expected_revision: u64,
        candidate: JsonValue,
    ) -> Result<JsonValue, String> {
        let candidate: InputMapDocument =
            serde_json::from_value(candidate).map_err(|error| format!("candidate: {error}"))?;
        service(&this)
            .apply_user_document(expected_revision, candidate)
            .await
            .map(|document| json(&document))
            .map_err(|error| error.to_string())
    }

    /// Calls `listener` with the state after every change, until the function it returns is
    /// called (or the reactive owner of the call goes), or this object is disposed.
    #[ts("(listener: (state: InputMapState) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(state)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service
                    .input_map()
                    .subscribe(move |state| post(json(&**state))),
            )
        })
    }
}

impl InputMapService {
    /// The service, for Rust bridges that take an `Instance<InputMapService>`.
    pub fn service(&self) -> &Arc<dyn InputMap> {
        &self.service
    }
}
