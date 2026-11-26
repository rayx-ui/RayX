# Commands and contexts

App-wide active context tracking (`ContextService`) and a command registry with contextual handler dispatch,
single-flight execution guards (`IgnoreWhileRunning`), and optional input-map shortcut resolution (`CommandService`).

## Register

| | |
| --- | --- |
| Traits | `rayx_app::services::commands::{ContextService, CommandService}` (`rayx::app::services::commands`) |
| Crate | `rayx_services_commands` (`rayx::services::commands`) |
| Feature | `commands` |
| Rust | `add_commands()` (`CommandsAppBuilderExt`) |
| TS | `addCommands()`, and the classes `ContextService`, `CommandService` |
| Optional | `dyn InputMapService` (`add_input_map(..)`) for `shortcutFor` and `resolveActiveBindings` |

```rust
use rayx_services_commands::CommandsAppBuilderExt as _;

let builder = RayXAppBuilder::new().add_commands();
// TS: new RayXAppBuilder().addCommands()
```

`add_commands()` registers RayX's implementations as both `dyn ContextService` (`RayxContext`) and `dyn CommandService`
(`RayxCommands`), replacing any earlier registrations of them; neither is a startup service. If the app also registers a
`dyn InputMapService` ([input map](input-map.md)), before or after `add_commands()`, the command service observes it for
shortcut lookup and active keybinding resolution. Without an input map, commands and contexts work standalone.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `commands` feature on, or
`rayx_services_commands::register(registry)` by hand.

## The contracts

### `ContextService` (`ContextServiceContract` in TS)

Tracks which UI contexts are currently active, innermost first. The empty context `""` represents the global context and
is always implicitly active last.

Published state:

- `context()` (`context`): a `StateCell<ContextState>`, `{ contexts: string[], revision: number }`, published only when
  the active context list changes.

Commands (`async`, `Result<…, CommandError>`; promises in TS):

- `push_context(context)` (`pushContext`): pushes `context` as the innermost active context and returns a numeric token
  id.
- `pop_context(token)` (`popContext`): removes the pushed entry for `token`; returns whether it was present.
- `set_contexts(contexts)` (`setContexts`): replaces the declarative base contexts (innermost first); pushed contexts
  remain ahead of declarative base contexts, and duplicates are removed preserving innermost-first order.

Rust helpers on `dyn ContextService`:

- `active_contexts()`: current active context names, innermost first.
- `has_context(ctx)`: whether `ctx` is active (`""` is always `true`).
- `subscribe(|contexts| ..)`: hears every change of the active context list until dropped.

### `CommandService` (`CommandServiceContract` in TS)

Published state and events:

- `commands()` (`commands`): a `StateCell<CommandState>`, `{ commands: CommandMetadata[], revision: number }`.
- `events()` (`events`): an `EventStream<CommandEvent>`, emitting `Registered(id)`, `Unregistered(id)`,
  `Executed { id }`, and `Failed { id, message }`.

Commands (`async`, `Result<bool, CommandError>`; promises in TS):

- `execute(id, arguments)` (`execute`): runs command `id` with optional JSON `arguments`. Returns `Ok(true)` if the
  command ran, `Ok(false)` if it is currently disabled (`can_execute` is `false` or out of context), or `Err(...)` if
  the command is unknown (`UnknownCommand`), already executing (`AlreadyExecuting`), or its handler failed
  (`ExecutionFailed`).

Rust helpers on `dyn CommandService`:

- `all()`, `get(id)`, `has(id)`, `can_execute(id)`, `is_executing(id)`, `title(id)`, `checked(id)`.
- `subscribe(|state| ..)` and `subscribe_events(|event| ..)`.

## Direct commands and contextual handlers

`RayxCommands` supports two complementary registration models:

1. **Direct commands** (`register`): register a command by unique `id` with a static or dynamic `title`, optional
   `category`, `kind`, `contexts`, `can_execute`, `checked`, and `run` handler. If `contexts` is non-empty, the command
   can execute only while at least one of those contexts is active.
2. **Contextual handlers** (`register_handler` / `registerHandler`): register a handler for a shared command `id` (such
   as `"edit.undo"` or `"edit.redo"`) scoped to a specific `context` (or `""` for the global fallback). Dispatch checks
   active contexts innermost-first, falling back to `""`, then to any matching direct command.

Each command enforces `IgnoreWhileRunning` per command id: overlapping executions reject with `AlreadyExecuting` while
`is_executing` is `true`, and cancellation or failure always resets `is_executing` back to `false`.

## Use (Rust)

```rust
use rayx::app::services::commands::{CommandService, ContextService};
use rayx::services::commands::{CommandDefinition, ContextCommandHandlerDefinition, RayxCommands, RayxContext};

let context = services.required_service::<RayxContext>()?;
let commands = services.required_service::<RayxCommands>()?;
let dyn_commands = services.required_service::<dyn CommandService>()?;

let _save = commands.register(
    CommandDefinition::sync("file.save", "Save", |_args| Ok(()))
        .with_category("File"),
)?;

let _undo_global = commands.register_handler(
    "edit.undo",
    "",
    ContextCommandHandlerDefinition::sync(|_| Ok(())).with_title("Undo"),
)?;
let _undo_editor = commands.register_handler(
    "edit.undo",
    "editor",
    ContextCommandHandlerDefinition::sync(|_| Ok(())).with_title("Undo Typing"),
)?;

let _guard = context.enter("editor")?; // popped on drop
dyn_commands.execute("edit.undo".into(), None).await?;
```

## TSX

`ContextService` and `CommandService`, resolved from the provider `start()` gives, wrap whichever `dyn ContextService`
and `dyn CommandService` are registered:

### `ContextService`

- `activeContexts()`, `hasContext(ctx)`, `state()`;
- `enter(ctx)`: pushes `ctx` synchronously and returns `() => void` to leave it (also cleans up automatically with the
  calling reactive owner);
- `setContexts(ctxs)`: replaces declarative base contexts;
- `pushContext(ctx)`, `popContext(token)`;
- `onChange((state) => ..)`: hears every context change until stopped.

### `CommandService`

- `register(descriptor)`: registers a direct command (`{ id, title, category?, kind?, contexts?, canExecute?, checked?, run }`)
  and returns `() => void` to unregister (also unregisters on reactive owner cleanup); `title`, `canExecute`, and
  `checked` accept reactive signal accessors;
- `registerHandler(id, context, handler)`: registers a contextual handler (`{ title?, category?, kind?, canExecute?, checked?, run }`)
  and returns `() => void`;
- `execute(id, args?)`: promise resolving `true` if ran, `false` if disabled, or rejecting on unknown / already
  executing / handler error;
- `canExecute(id)`, `isExecuting(id)`, `title(id)`, `checked(id)`, `all()`, `has(id)`, `refresh()`, `state()`;
- `shortcutFor(id)`: resolves the preferred NSH `+`-syntax keyboard shortcut (e.g. `"mod+z"`) from `InputMapService`
  and active contexts, or `null`;
- `resolveActiveBindings()`: computes the deduplicated active NSH keymap bindings
  (`{ keystroke, action, arguments?, context, when?, repeat, passThrough }[]`) for the current contexts and input map;
- `onChange((state) => ..)` and `onEvent((event) => ..)`.

```tsx
async function setupCommands(services: ServiceProvider) {
  const contexts = services.getRequiredService(ContextService);
  const commands = services.getRequiredService(CommandService);

  const leave = contexts.enter("editor");
  const unreg = commands.register({
    id: "file.save",
    title: "Save",
    category: "File",
    canExecute: () => true,
    run: async () => { /* save work */ },
  });

  await commands.execute("file.save");
  const hint = commands.shortcutFor("file.save"); // e.g. "mod+s"
  const bindings = commands.resolveActiveBindings();

  unreg();
  leave();
}
```

## Replace it

An app may register its own `dyn ContextService` or `dyn CommandService` in place of `add_commands()`, in Rust or in TS
by implementing `ContextServiceContract` / `CommandServiceContract` and registering under `ContextService` /
`CommandService`.
