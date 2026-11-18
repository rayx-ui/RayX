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
time (a save nothing awaits that fails is a `NavigationFailed` event of the scope). A save that finds another revision
in the store than it expects (another app's, or a save whose answer was lost) reloads it and saves over it. Changes
made before the app's startup attaches the store are saved once it does. Its `diagnostics` feature keeps the replays
that failed in the published state.

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
  aborted before it commits (`StackChanged`). `clear_scope` discards the operations of transactions begun before it
  and committed after it.
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
