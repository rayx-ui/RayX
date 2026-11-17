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
`dyn KeyValueStore`) and registers none, so it persists wherever the store does: on NSH, in the device's storage.
`HistoryStore` (in `rayx_services_history`) is the implementation's extension point: where it keeps each scope's
journal (`HistoryJournalV2`), with `InMemoryHistoryStore` and `KeyValueHistoryStore`. Its `diagnostics` feature keeps
the replays that failed in the published state.

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
  by resubmitting their inverse operations (or the operations), keeping the inverses the replay answered.
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
