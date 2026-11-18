# History

Undo and redo per history scope, over the [operation service](operations.md).

## Register

Pick where the history lives:

```rust
use rayx_services_history::HistoryAppBuilderExt;

builder.add_history()                     // in memory
builder.add_persistent_history()          // in the app's KeyValueStore (KeyValueHistoryStore)
builder.add_history_with_store(my_store)  // any HistoryStore
```

The contract is `dyn HistoryService` (`rayx_app::services::history`): its state (each scope's depths), its events, its
commands and data types, NSH-free and implementable in Rust or TS. RayX's implementation is `rayx_services_history`'s
(`builder.addHistory()` and `builder.addPersistentHistory()` in TS, once its set is registered with
`rayx_services_history::register(registry)`), registered as the app's `dyn HistoryService`, replacing any other (an
app's own, registered after it, wins). It is a startup service observing the app's `dyn OperationService`, which it
requires and registers none of: `add_operations()` from `rayx_services_operations`, or the app's own; the build names
it when it is missing.

`add_persistent_history()` keeps each scope's journal as JSON in the `rayx.history` bucket of the app's
[key-value store](storage.md), which it requires (`add_storage()` from `rayx_services_storage`, or the app's own
`dyn KeyValueStore`) and registers none, so it persists wherever the store does: on NSH, in the device's storage, or
in a TS implementation's, which answers in JS's next turn. `HistoryStore` (in `rayx_services_history`) is the
implementation's extension point: where it keeps each scope's journal (`HistoryJournalV2`), with
`InMemoryHistoryStore` and `KeyValueHistoryStore`. Its `load`, `save` and `delete_scope` return futures, answered at
once or later: the history loads a scope when a command first awaits it and saves each change behind it, one save at a
time. A failure nobody awaits (a save behind an edit, a command whose caller gave up or dropped its answer unread) is a
`NavigationFailed` event of the scope. Entries have ids, given when they are recorded and saved with the journal (an
entry of a journal saved before ids existed gets one when it loads). Histories may share a store (two windows, two app
instances): a save that finds another journal in the store than it expects reloads it and merges this history's
entries in by id (the stored entries, less those this history saved and dropped since, each of its own replaced by
its current copy, then its new ones), so no writer's entries are lost. Each session's undo and redo stacks keep only
its own entries (and those it loaded at its start): another session's stay in the store for that session. Changes
made before the app's startup attaches the store are merged with the stored journal the same way once it does. Its
`diagnostics` feature keeps the replays that failed in the published state.

Each call the history makes to begin a transaction, submit a record or reach the store is bounded by
`HistoryOptions::call_timeout_ms` (bound from `RayX:Services:History`, `configure_options::<HistoryOptions>(..)`; 30
seconds by default, `None` for none): one that does not answer in time (a TS promise that never settles) fails the
command with `HistoryError::TimedOut`, aborting its transaction and freeing its scope. A TS operation service or store
that legitimately takes longer than the timeout has its calls aborted: raise the timeout for it, or set `None`
(`null` in TS) for none. A commit is never cut short: once asked, its change lands, so the scope stays busy until it
answers and the stacks then follow it. A commit that never answers therefore keeps its scope busy (`ScopeBusy`) for
the app's life: the price of never undoing a change twice. The operation service recovers the scope by answering the
commit, or by failing it (rejecting its promise, in TS): the replay then fails and frees the scope. The timer is the engine's on NSH (through the host's `EngineRunner`);
elsewhere an app registers a `HistoryTimer` service, and without one the calls are unbounded. In TS the options are
set by the builder methods' callback, after configuration's:
`builder.addHistory((options) => options.callTimeoutMs(5000))` (or `addPersistentHistory(..)`; `null` for no limit).

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

- The history records the operations the local user commits through the app's operation service (a transaction is
  one entry) in the scope their target is bound to, else in the scope of the target's id, and undoes and redoes them
  by resubmitting their inverse operations (or the operations) in a transaction of its own, carried by its id, keeping
  the inverses the replay answered. It works over any `dyn OperationService`, one answering later (a TS one) included,
  from any task or thread.
- A scope is busy (`ScopeBusy`) while it is replayed: another undo, redo, `clear_scope` or `set_scope_entry_limit` of
  it fails meanwhile. An operation recorded meanwhile is kept, and a redo whose entry an edit cleared meanwhile is
  aborted before it commits (`StackChanged`). The stacks keep the order the changes happened in: an edit recorded
  while a redo's commit is pending stays above the redone entry, and one recorded while an undo's commit is pending
  clears the redo stack, the undone entry included. `clear_scope` discards the operations of transactions begun before
  it and committed after it.
- `undo`, `redo`, `clear_scope` and `set_scope_entry_limit` start when called and run to their end, their saves
  included: dropping the future gives up only the wait for the answer, never a change half made.
- `can_undo`, `can_redo`, `stack_depths` and `diagnostics` read the published state (`history()`), synchronously; a
  persisted scope shows once it is loaded (bound, replayed or recorded in).
- `clear_scope`, `set_scope_entry_limit` and `retained_records` keep the scopes' lifecycles; `subscribe(..)` (or
  `events()`) hears every `HistoryEvent`.
- An operation committed on another thread is recorded when its event is delivered, which may be after that thread's
  `submit` returned (while another thread delivers an earlier event).

In TS, `services.getRequiredService(HistoryService)` wraps whichever `dyn HistoryService` is registered: `canUndo`,
`canRedo` and `depths` answer at once, `bindTargetScope`, `undo`, `redo`, `clearScope`, `setScopeEntryLimit` and
`retainedRecords` return promises, and `onChange` hears every event.

`rayx_app::testing::FakeHistory` is a fake of the contract, for testing what consumes the history.
