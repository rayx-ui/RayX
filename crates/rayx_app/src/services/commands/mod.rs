//! Commands and contexts: app-wide active context tracking ([`ContextService`]) and a registry of
//! executable commands with contextual handler dispatch and concurrency guards
//! ([`CommandService`]).
//!
//! RayX's implementations live in `rayx_services_commands` (`add_commands()`); an app may register
//! its own implementations in Rust or in TS. Reading published state ([`ContextService::context`],
//! [`CommandService::commands`]) and synchronous helpers on `dyn ContextService` and
//! `dyn CommandService` work identically for any implementation.

mod error;
mod model;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError};

pub use error::CommandError;
pub use model::{ActiveKeyBinding, CommandEvent, CommandMetadata, CommandState, ContextState};

use crate::ChangeSubscription;
use crate::published::{EventStream, StateCell};

/// Tracks the app's active contexts (innermost first), published as [`ContextState`].
///
/// The empty context `""` represents the global context and is always implicitly active last.
#[rayx_app_macros::contract(ts = "ContextServiceContract")]
pub trait ContextService {
    /// The active contexts (innermost first) and revision, now and as they change.
    #[state(ts = "ContextState")]
    fn context(&self) -> &StateCell<ContextState>;

    /// Pushes `context` onto the active context stack (innermost) and returns a token id that
    /// removes it when passed to [`ContextService::pop_context`].
    #[ts("(context: string): number")]
    async fn push_context(&self, context: String) -> Result<u64, CommandError>;

    /// Removes the pushed context identified by `token`; returns whether it was present.
    #[ts("(token: number): boolean")]
    async fn pop_context(&self, token: u64) -> Result<bool, CommandError>;

    /// Replaces the declarative base contexts (innermost first). Pushed contexts remain ahead of
    /// the declarative base contexts.
    #[ts("(contexts: string[])")]
    async fn set_contexts(&self, contexts: Vec<String>) -> Result<(), CommandError>;
}

impl dyn ContextService {
    /// The active context names now, innermost first (never containing `""`).
    pub fn active_contexts(&self) -> Vec<String> {
        self.context().get().contexts.clone()
    }

    /// Whether `context` is currently active (`""` is always active).
    pub fn has_context(&self, context: &str) -> bool {
        self.context().get().has_context(context)
    }

    /// Calls `listener` with the updated active context list whenever it changes, until the
    /// subscription is dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(&[String]) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        let initial = self.context().get().contexts.clone();
        let last = Mutex::new(initial);
        self.context().subscribe(move |state: &Arc<ContextState>| {
            let changed = {
                let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
                if *last == state.contexts {
                    false
                } else {
                    *last = state.contexts.clone();
                    true
                }
            };
            if changed {
                let _ = catch_unwind(AssertUnwindSafe(|| listener(&state.contexts)));
            }
        })
    }
}

/// The command registry and execution service, published as [`CommandState`] and [`CommandEvent`]s.
///
/// Commands are identified by string ids (for example `"edit.undo"`, `"file.save"`), resolve
/// contextual handlers innermost-context-first against the active [`ContextService`], and enforce
/// single-flight execution per command id.
#[rayx_app_macros::contract(ts = "CommandServiceContract")]
pub trait CommandService {
    /// The registered commands and their current metadata, now and as they change.
    #[state(ts = "CommandState")]
    fn commands(&self) -> &StateCell<CommandState>;

    /// Events emitted as commands are registered, unregistered, executed, or fail.
    #[events(ts = "CommandEvent")]
    fn events(&self) -> &EventStream<CommandEvent>;

    /// Executes command `id` with optional JSON `arguments`.
    ///
    /// Returns `Ok(true)` if the command ran, `Ok(false)` if it is currently disabled
    /// (`can_execute` is false or out of context), or `Err(...)` if the command is unknown,
    /// already executing, or its handler failed.
    #[ts("(id: string, arguments: unknown | null): boolean")]
    async fn execute(
        &self,
        id: String,
        arguments: Option<serde_json::Value>,
    ) -> Result<bool, CommandError>;
}

impl dyn CommandService {
    /// All registered commands' current metadata, sorted by `id`.
    pub fn all(&self) -> Vec<CommandMetadata> {
        self.commands().get().commands.clone()
    }

    /// Current metadata for command `id`, if registered.
    pub fn get(&self, id: &str) -> Option<CommandMetadata> {
        self.commands().get().get(id).cloned()
    }

    /// Whether a command with `id` is registered.
    pub fn has(&self, id: &str) -> bool {
        self.commands().get().contains(id)
    }

    /// Whether command `id` is registered and can currently execute.
    pub fn can_execute(&self, id: &str) -> bool {
        self.commands().get().can_execute(id)
    }

    /// Whether command `id` is currently executing.
    pub fn is_executing(&self, id: &str) -> bool {
        self.commands().get().is_executing(id)
    }

    /// Current title of command `id`, if registered.
    pub fn title(&self, id: &str) -> Option<String> {
        self.commands()
            .get()
            .get(id)
            .map(|command| command.title.clone())
    }

    /// Current checked state of command `id`, if registered and checkable.
    pub fn checked(&self, id: &str) -> Option<bool> {
        self.commands()
            .get()
            .get(id)
            .and_then(|command| command.checked)
    }

    /// Calls `listener` whenever the published [`CommandState`] changes, until the subscription is
    /// dropped.
    pub fn subscribe(
        &self,
        listener: impl Fn(&CommandState) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.commands().subscribe(move |state: &Arc<CommandState>| {
            let _ = catch_unwind(AssertUnwindSafe(|| listener(state)));
        })
    }

    /// Calls `listener` with every [`CommandEvent`], until the subscription is dropped.
    pub fn subscribe_events(
        &self,
        listener: impl Fn(&CommandEvent) + Send + Sync + 'static,
    ) -> ChangeSubscription {
        self.events().subscribe(move |event| {
            let _ = catch_unwind(AssertUnwindSafe(|| listener(event)));
        })
    }
}
