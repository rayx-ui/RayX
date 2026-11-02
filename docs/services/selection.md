# Selection

The app-wide summary of the active selection area: which area publishes, and how much it has selected, never the
selected content.

## Register

```rust
builder.add_navigation(|_| {}).add_selection()
```

The selection follows the [navigation service](navigation.md), registered before or after it; startup fails without
one.

## Use

```rust
let selection = services.required_service::<SelectionService>()?;
selection.activate(area, status);       // this area publishes now
selection.publish_status(area, status); // its status changed
selection.deactivate(area);
```

- An area is a `SelectionAreaId`; its `SelectionStatus` holds the participants, the selected characters, whether it is
  collapsed and whether a selection is in progress.
- Only the active area publishes, and every route change clears it.
- `subscribe(..)` hears every change; the revisions (`active_area_revision`, `status_revision`) say what changed.
