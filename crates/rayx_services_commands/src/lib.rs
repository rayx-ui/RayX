//! RayX's context and command services: active context stacks ([`RayxContext`]), contextual
//! command dispatch with single-flight execution guards ([`RayxCommands`]), and optional
//! `InputMapService` shortcut and active-keybinding resolution.
//!
//! - Rust: `RayXAppBuilder::new().add_commands()` ([`CommandsAppBuilderExt`]), then resolve
//!   `dyn ContextService`, `dyn CommandService`, [`RayxContext`], or [`RayxCommands`].
//! - TS: `new RayXAppBuilder().addCommands()`, with the classes [`ContextService`] and
//!   [`CommandService`] over whichever `dyn ContextService` / `dyn CommandService` is registered.
//! - The app's set: [`register`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod builder;
mod commands_ts;
mod context;
mod keymap;
mod service;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::services::commands::*;
use rayx_app::{EventStream, StateCell};
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use builder::{CommandsAppBuilderExt, CommandsBuilder};
pub use commands_ts::{CommandService, ContextService};
pub use context::{ContextGuard, RayxContext};
pub use keymap::{input_sequence_to_nsh_keystroke, resolve_active_bindings, shortcut_for};
pub use service::{
    BoolProvider, CheckedProvider, CommandAction, CommandDefinition, CommandRegistration,
    ContextCommandHandlerDefinition, RayxCommands, TitleProvider,
};

rayx_app_nsh::contract_nsh!(rayx_app::services::commands::ContextService as ContextContract);
rayx_app_nsh::contract_nsh!(rayx_app::services::commands::CommandService as CommandContract);

/// The active context state in TS.
pub const CONTEXT_STATE: TsAlias = TsAlias {
    name: "ContextState",
    ts: "{ contexts: string[]; revision: number }",
    doc: "The active context names (innermost first, excluding global `\"\"`) and revision: the JSON of Rust's `ContextState`.",
};

/// A registered command's metadata in TS.
pub const COMMAND_METADATA: TsAlias = TsAlias {
    name: "CommandMetadata",
    ts: "{ id: string; title: string; category?: string | null; kind?: string | null; contexts: string[]; can_execute: boolean; is_executing: boolean; checked?: boolean | null }",
    doc: "Metadata describing a registered command and its current state: the JSON of Rust's `CommandMetadata`.",
};

/// The command registry's state in TS.
pub const COMMAND_STATE: TsAlias = TsAlias {
    name: "CommandState",
    ts: "{ commands: CommandMetadata[]; revision: number }",
    doc: "The registered commands (sorted by id) and revision: the JSON of Rust's `CommandState`.",
};

/// A command lifecycle event in TS.
pub const COMMAND_EVENT: TsAlias = TsAlias {
    name: "CommandEvent",
    ts: "{ Registered: string } | { Unregistered: string } | { Executed: { id: string } } | { Failed: { id: string; message: string } }",
    doc: "An event emitted by `CommandService`: the JSON of Rust's `CommandEvent`.",
};

/// A direct command descriptor accepted by `CommandService.register`.
pub const COMMAND_DESCRIPTOR: TsAlias = TsAlias {
    name: "CommandDescriptor",
    ts: "{ id: string; title: string | (() => string); category?: string | null; kind?: string | null; contexts?: string[]; canExecute?: () => boolean; checked?: () => boolean | null | undefined; run: (args?: unknown) => void | Promise<void> }",
    doc: "Descriptor passed to `CommandService.register(descriptor)` to register a direct command.",
};

/// A contextual handler accepted by `CommandService.registerHandler`.
pub const CONTEXT_COMMAND_HANDLER: TsAlias = TsAlias {
    name: "ContextCommandHandler",
    ts: "{ title?: string | (() => string); category?: string | null; kind?: string | null; canExecute?: () => boolean; checked?: () => boolean | null | undefined; run: (args?: unknown) => void | Promise<void> }",
    doc: "Handler object passed to `CommandService.registerHandler(id, context, handler)` for a contextual command override.",
};

/// An active keyboard binding resolved for NSH keymaps in TS.
pub const ACTIVE_KEY_BINDING: TsAlias = TsAlias {
    name: "ActiveKeyBinding",
    ts: "{ keystroke: string; action: string; arguments?: unknown; context: string; when?: \"always\" | \"auto\" | \"notEditing\" | null; repeat: boolean; passThrough: boolean }",
    doc: "A deduplicated active NSH keymap binding resolved from `ContextService` and `InputMapService`: the JSON of Rust's `ActiveKeyBinding`.",
};

/// Adds the context and command services' TS side to an app's set: the [`ContextService`] and
/// [`CommandService`] classes, their contracts (`ContextServiceContract`, `CommandServiceContract`),
/// their type aliases, and `builder.addCommands()`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(CONTEXT_STATE)
        .ts_alias(COMMAND_METADATA)
        .ts_alias(COMMAND_STATE)
        .ts_alias(COMMAND_EVENT)
        .ts_alias(COMMAND_DESCRIPTOR)
        .ts_alias(CONTEXT_COMMAND_HANDLER)
        .ts_alias(ACTIVE_KEY_BINDING)
        .contract_class::<ContextContract, ContextService>()
        .contract_class::<CommandContract, CommandService>()
        .class_extension::<CommandsBuilder>();
}
