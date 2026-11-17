# Selection

The app-wide summary of the active selection area: which area publishes, and how much it has selected, never the
selected content.

## Register

```rust
use rayx_services_navigation::NavigationAppBuilderExt;
use rayx_services_selection::SelectionAppBuilderExt;

builder.add_navigation(|_| {}).add_selection()
```

The contract is `dyn SelectionService` (`rayx_app::services::selection`): its state, commands and data types, NSH-free
and implementable in Rust or TS. RayX's implementation is `rayx_services_selection`'s (`builder.addSelection()` in TS,
once its set is registered with `rayx_services_selection::register(registry)`), registered as the app's
`dyn SelectionService`, replacing any other (an app's own, registered after it, wins).

It follows the app's `dyn NavigationService` ([navigation](navigation.md): `add_navigation(..)` from
`rayx_services_navigation`, or the app's own), registered before or after it, and clears on every route change. It
requires one: without it the app does not build, and the error names `add_navigation(..)`.

## Use

```rust
let selection = services.required_service::<dyn SelectionService>()?;
selection.activate(area, status).await?;       // this area publishes now
selection.publish_status(area, status).await?; // its status changed
selection.deactivate(area).await?;
```

- An area is a `SelectionAreaId`; its `SelectionStatus` holds the participants, the selected characters, whether it is
  collapsed and whether a selection is in progress.
- Only the active area publishes, and every route change clears it.
- The state, `selection()` (a `SelectionState`: the active area, its status and the revisions), is published only when
  it changes; `active_area()`, `status()` and the revisions (`navigation_revision`, `active_area_revision`,
  `status_revision`) read it, and `subscribe(..)` hears every change of the area or its status.
- RayX's commands take effect before they return; await them as any implementation's.

## TSX

```tsx
const selection = services.getRequiredService(SelectionService);
await selection.activate(3, { participant_count: 1, selected_character_count: 4, is_collapsed: false, is_selecting: false });
selection.activeArea(); // 3
const stop = selection.onChange((state) => show(state.status));
```

The class `SelectionService` (the contract's token) wraps whichever `dyn SelectionService` is registered: `state()`,
`activeArea()`, `status()`, `onChange(listener)`, and, as promises, `activate`, `publishStatus`, `deactivate` and
`clear`. A selection service written in TS implements `SelectionServiceContract` and registers under the class.
