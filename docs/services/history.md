# History

Undo and redo per history scope, over the [operation service](operations.md).

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::history::HistoryService` (`rayx::app::services::history`) |
| Crate | `rayx_services_history` (`rayx::services::history`) |
| Feature | `history` |
| Rust | `add_history()`, `add_history_with_store(store)`, `add_persistent_history()` (`HistoryAppBuilderExt`) |
| TS | `addHistory(configure?)`, `addPersistentHistory(configure?)`, `addHistoryWithStore(store, configure?)`, and the classes `HistoryService` and `HistoryOptions` |
| Requires | `dyn OperationService` (`add_operations()`); `add_persistent_history()` also `dyn KeyValueStore` (`add_storage()`) |

Pick where the history lives:

```rust
use rayx::prelude::*; // `HistoryAppBuilderExt`, with the `history` feature

builder.add_operations().add_history()                          // in memory
builder.add_operations().add_storage().add_persistent_history() // in the app's KeyValueStore
builder.add_operations().add_history_with_store(my_store)       // any HistoryStore
```

<!-- tsx: fragment -->
```tsx
new RayXAppBuilder().addOperations().addHistory();
new RayXAppBuilder().addOperations().addStorage().addPersistentHistory((options) => options.callTimeoutMs(5000));
new RayXAppBuilder().addOperations().addHistoryWithStore(() => new MyStore()); // a store written in TS (below)
```

Each registers RayX's implementation as the app's `dyn HistoryService`, replacing any earlier registration of it: a
startup service that initializes after the app's `dyn OperationService` (and its store, when it has one) and observes
it. It registers neither the operation service nor the key-value store: building without one fails, naming the
missing service and what registers it. In TS, `addHistory`, `addPersistentHistory` and `addHistoryWithStore` are there
once the crate's set is registered (`#[rayx::native_module]` with the feature on, or
`rayx_services_history::register(registry)`).

### Where it keeps the history

`add_persistent_history()` keeps each scope's journal as JSON in the `rayx.history` bucket (`HISTORY_BUCKET`) of the
app's [key-value store](storage.md), through a `KeyValueHistoryStore`, so it persists wherever the store does: on NSH,
in the device's storage, or in a TS implementation's, which answers in JS's next turn. `add_history_with_store(store)`
registers `store` as the app's `dyn HistoryStore`.

In TS, `addHistoryWithStore(() => new MyStore())` keeps the journals in a store written in TS: a class implementing
`HistoryStoreContract` (Rust's contract `HistoryJournalStore`), made by `start()`, which keeps each scope's journal as
JSON text by revision; RayX's history keeps its journals in it through a `HistoryStore` over it:

```tsx
class MyStore implements HistoryStoreContract {
  private journals = new Map<string, { text: string; revision: number }>();
  async load(scope: string) { return this.journals.get(scope)?.text ?? null; }
  async save(scope: string, journal: string, revision: number, expectedRevision: number | null) {
    if ((this.journals.get(scope)?.revision ?? null) !== expectedRevision) return false; // a conflict
    this.journals.set(scope, { text: journal, revision });
    return true;
  }
  async deleteScope(scope: string, expectedRevision: number) {
    if ((this.journals.get(scope)?.revision ?? null) !== expectedRevision) return false; // a conflict
    this.journals.delete(scope);
    return true;
  }
}
```

What its methods throw is the store's `Backend` error.

`HistoryStore` (in `rayx_services_history`) is the implementation's extension point, a startup service the history
waits for: where it keeps each scope's journal (`HistoryJournalV2`), with `InMemoryHistoryStore` and
`KeyValueHistoryStore`. Its `load`, `save` and `delete_scope` return futures, answered at once or later; `save` and
`delete_scope` take the revision the history expects in the store and check it and write in one step (a save fails
with `Conflict`, a delete answers `false`), so a journal another history saved meanwhile is never overwritten or
deleted. `InMemoryHistoryStore` makes the step under its own lock. `KeyValueHistoryStore` makes it under the
process's one lock of the scope's journal (its bucket and the scope's id), whatever `dyn KeyValueStore` object it goes
through, so it holds for the histories of every window, surface and app instance of one process: the surfaces over
one NSH storage each have their own NSH store (`add_storage()`), and NSH shares the storage's entries between them.
Histories over unrelated stores that use one scope id take turns at the step. Across processes it does not hold: a
`KeyValueStore` has no compare step (none of RayX's has one), so two processes writing one scope's journal into one
storage can overwrite or delete each other's. Those histories need a store of the app's with a real compare-and-swap
(a `HistoryStore`, or a TS `HistoryStoreContract`, over a backend that offers one).
The history loads a scope when a command first awaits it and saves each change behind
it, one save at a time. A failure nobody awaits (a save behind an edit, a command whose caller gave up or dropped its
answer unread) is a `NavigationFailed` event of the scope. Entries have ids, given when they are recorded and saved
with the journal (an entry of a journal saved before ids existed gets one when it loads). Histories may share a store
(two windows, two app instances of one process, over one key-value store): a save that finds another journal in the
store than it expects reloads it and merges this history's entries in by id (the stored entries, less those this
history saved and dropped since, each of its own replaced by its current copy, then its new ones), so no writer's
entries are lost. Each session's undo and redo stacks keep only its own entries (and those it loaded at its start):
another session's stay in the store for that session. Changes made before the app's startup attaches the store are
merged with the stored journal the same way once it does.

The crate's own `diagnostics` feature keeps the replays that failed in the published state.

## Configure

`HistoryOptions` (`rayx_services_history::HistoryOptions`) bind from `RayX:Services:History` (`HISTORY_CONFIG_PATH`);
Rust changes them with `configure_options::<HistoryOptions>(..)`, TS with the builder methods' callback, after
configuration's (`options.callTimeoutMs(5000)`, `null` for no limit; `options.recordAgentOperations(true | false)`; a
callback that throws makes the call throw).

```json
{ "RayX": { "Services": { "History": { "callTimeoutMs": 5000, "recordAgentOperations": true } } } }
```

`record_agent_operations` (`recordAgentOperations` in TS) controls whether operations with `OperationOrigin::Agent(_)`
are recorded as undoable history entries alongside `OperationOrigin::LocalUser` (`true` by default).

`call_timeout_ms` bounds each call the history makes to begin a transaction, submit a record or reach the store (30
seconds by default, `None` for none): one that does not answer in time (a TS promise that never settles) fails the
command with `HistoryError::TimedOut`, aborting its transaction and freeing its scope. A TS operation service or store
that legitimately takes longer than the timeout has its calls aborted: raise the timeout for it, or set `None` (`null`
in TS). A commit is never cut short: once asked, its change lands, so the scope stays busy until it answers and the
stacks then follow it. A commit that never answers therefore keeps its scope busy (`ScopeBusy`) for the app's life: the
price of never undoing a change twice. The operation service recovers the scope by answering the commit, or by failing
it (rejecting its promise, in TS): the replay then fails and frees the scope.

## The contract

`HistoryService` (TS: `HistoryServiceContract`) is NSH-free and implementable in Rust or TS.

Published state:

- `history()` (TS `history`): a `HistoryState`, each loaded scope's undo and redo depths, top undo/redo labels
  (`undo_label`, `redo_label`), revision and entry limit, and the diagnostics (replays that met a released target, the
  latest failure codes).

Events:

- `events()` (TS `events`): every `HistoryEvent`: `Recorded(entry)`, `ScopeCleared`, `AvailabilityChanged` (the
  limit changed), `UndoCompleted`, `RedoCompleted`, `NavigationFailed`.

Commands:

- `bind_target_scope(target, scope)` (TS `bindTargetScope`): records the target's operations in the scope from now
  on, and loads the scope's history.
- `undo(scope)`, `redo(scope)` (TS the same): replay the latest entry; whether there was one.
- `clear_scope(scope)` (TS `clearScope`): clears both stacks, keeping the binding and limit; with a store, the scope's
  journal is deleted from it (`delete_scope`, TS `deleteScope`, at the revision this history expects), unless another
  history's entries are in it, which are saved without this history's: a delete that finds another journal (another
  history saved meanwhile) reloads it and merges, as a save does. A clear made while the delete runs leaves the scope
  deleted, never saved empty.
- `set_scope_entry_limit(scope, limit)` (TS `setScopeEntryLimit`): bounds the scope's entries (at least 1), the oldest
  undo entries going first, then the most distant redo entries.
- `retained_records(scope)` (TS `retainedRecords`): the `OperationRecord`s the scope keeps, newest undo entries first,
  then newest redo entries.

Rust helpers (on `dyn HistoryService`), synchronous, over the published state:

- `can_undo(&scope)`, `can_redo(&scope)`, `stack_depths(&scope)`, `undo_label(&scope)`, `redo_label(&scope)`,
  `diagnostics(&scope)`;
- `subscribe(..)`: hears every event, isolating a subscriber that panics.

## Use

```rust
let history = services.required_service::<dyn HistoryService>()?;
let editor = HistoryScopeId::new("editor")?;
// Record this target's operations in "editor" (and load the scope's persisted history).
history.bind_target_scope(document_target, editor.clone()).await?;

if history.can_undo(&editor) {
    history.undo(editor.clone()).await?;
}
history.redo(editor).await?;
```

- The history records the operations the local user (`OperationOrigin::LocalUser`) and, when
  `record_agent_operations` is enabled (the default), an agent (`OperationOrigin::Agent(_)`) commit through the app's
  operation service (a transaction is one entry, whose `label_key` comes from the first receipt's
  `forward.metadata.label_key` when present, else `forward.kind`) in the scope their target is bound to, else in the
  scope of the target's id, and undoes and redoes them by resubmitting their inverse operations (or the operations) in
  a transaction of its own, carried by its id, keeping the inverses the replay answered. It works over any
  `dyn OperationService`, one answering later (a TS one) included, from any task or thread.
- A scope is busy (`ScopeBusy`) while it is replayed: another undo, redo, `clear_scope` or `set_scope_entry_limit` of
  it fails meanwhile. An operation recorded meanwhile is kept, and a redo whose entry an edit cleared meanwhile is
  aborted before it commits (`StackChanged`). The stacks keep the order the changes happened in: an edit recorded
  while a redo's commit is pending stays above the redone entry, and one recorded while an undo's commit is pending
  clears the redo stack, the undone entry included. `clear_scope` discards the operations of transactions begun before
  it and committed after it.
- `undo`, `redo`, `clear_scope` and `set_scope_entry_limit` start when called and run to their end, their saves
  included: dropping the future gives up only the wait for the answer, never a change half made.
- `can_undo`, `can_redo`, `stack_depths` and `diagnostics` read the published state, synchronously; a persisted scope
  shows once it is loaded (bound, replayed or recorded in).
- An operation committed on another thread is recorded when its event is delivered, which may be after that thread's
  `submit` returned (while another thread delivers an earlier event).

## On NSH

The call timeout runs on the engine's timers, through the host's `EngineRunner`. Elsewhere an app registers a
`HistoryTimer` service (`HistoryTimer::new(|duration| ..)`), and without one the calls are unbounded. A persistent
history's journals are in NSH's storage through the app's `dyn KeyValueStore` (`rayx_services_storage`'s, or a TS one).

## TSX

`HistoryService`, resolved from the provider `start()` gives, wraps whichever `dyn HistoryService` is registered:

- `canUndo(scope)`, `canRedo(scope)`, `depths(scope)` (`[undo, redo]`), `undoLabel(scope)` and `redoLabel(scope)`
  answer at once;
- `bindTargetScope(target, scope)`, `undo(scope)`, `redo(scope)`, `clearScope(scope)`, `setScopeEntryLimit(scope,
  limit)` and `retainedRecords(scope)` return promises that reject with the history's error;
- `onChange(listener)` hears every event (`{ Recorded: entry }`, `{ UndoCompleted: scope }`, ...); it returns a
  function that stops it.

```tsx
import { signal } from "nsh";

const undoEnabled = signal(false);

// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function bindEditor(services: ServiceProvider) {
  const history = services.getRequiredService(HistoryService);
  await history.bindTargetScope("doc", "editor");
  const stop = history.onChange(() => undoEnabled.set(history.canUndo("editor")));
  if (history.canUndo("editor")) await history.undo("editor");
}
```

## Replace it

Register the app's own `dyn HistoryService` instead of calling `add_history()` (it then needs no operation service
unless it uses one):

```rust
builder.add_singleton_as::<dyn HistoryService>(Arc::new(MyHistory::new()) as Arc<dyn HistoryService>)
```

To keep RayX's history but change where it keeps journals, pass a `HistoryStore` to `add_history_with_store`, or give
`add_persistent_history()` the app's own `dyn KeyValueStore`; to change what it replays through, register the app's
own `dyn OperationService` ([Operations](operations.md#replace-it)).

In TS, a class implementing `HistoryServiceContract`, registered under the class `HistoryService`
([contracts](../tsx.md#contracts)):

```tsx
class MyHistory implements HistoryServiceContract {
  history = new ContractState<HistoryState>({ scopes: {}, unavailable_entry_count: 0, recent_failure_codes: [] });
  events = new ContractEvents<HistoryEvent>();
  async bindTargetScope(targetId: string, scopeId: string) { /* ... */ }
  async undo(scopeId: string) {
    // ... then publish the new depths
    this.history.publish({ scopes: { [scopeId]: { undo_depth: 0, redo_depth: 1, revision: 2, entry_limit: null } },
      unavailable_entry_count: 0, recent_failure_codes: [] });
    this.events.publish({ UndoCompleted: scopeId });
    return true;
  }
  async redo(scopeId: string) { /* ... */ return true; }
  async clearScope(scopeId: string) { /* ... */ }
  async setScopeEntryLimit(scopeId: string, limit: number) { /* ... */ }
  async retainedRecords(scopeId: string) { return []; }
}

new RayXAppBuilder().addSingletonAs(HistoryService, () => new MyHistory());
```

Rust's `can_undo` and the class's `canUndo` then read its published state.

`rayx_app::testing::FakeHistory` is a fake of the contract, for testing what consumes the history.
