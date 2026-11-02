# History

Undo and redo per history scope, over the [operation service](operations.md).

## Register

Pick where the history lives:

```rust
builder.add_history()                     // in memory (InMemoryHistoryStore)
builder.add_persistent_history()          // in the app's KeyValueStore (KeyValueHistoryStore)
builder.add_history_with_store(my_store)  // any HistoryStore
```

Each registers the operation service too. `add_persistent_history()` keeps each scope's journal as JSON in the
`rayx.history` bucket of the app's [storage](storage.md), so it persists wherever the store does: on NSH, in the
device's storage.

## Use

```rust
let history = services.required_service::<HistoryService>()?;
let editor = HistoryScopeId::new("editor")?;
history.bind_target_scope(document_target, editor.clone())?; // record this target's receipts in "editor"

if history.can_undo(&editor)? {
    history.undo(&editor)?;
}
history.redo(&editor)?;
```

- `HistoryService` records the receipts of the targets bound to a scope and undoes and redoes them through their
  inverse operations.
- It enforces size limits, keeps scope lifecycles and publishes `HistoryEvent`s.
- `HistoryStore` persists each scope's journal (`HistoryJournalV2`).
