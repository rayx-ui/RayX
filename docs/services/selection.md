# Selection

The app-wide summary of the active selection area: which area publishes, and how much it has selected, never the
selected content.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::selection::SelectionService` (`rayx::app::services::selection`) |
| Crate | `rayx_services_selection` (`rayx::services::selection`) |
| Feature | `selection` |
| Rust | `add_selection()` (`SelectionAppBuilderExt`) |
| TS | `addSelection()`, and the class `SelectionService` |
| Requires | `dyn NavigationService` (`add_navigation(..)`) |

```rust
use rayx_services_navigation::NavigationAppBuilderExt as _;
use rayx_services_selection::SelectionAppBuilderExt as _;

let builder = RayXAppBuilder::new().add_navigation(|_| {}).add_selection();
// TS: new RayXAppBuilder().addNavigation().addSelection()
```

`add_selection()` registers RayX's implementation as the app's `dyn SelectionService`, replacing any earlier
registration of it; it is not a startup service. It follows the app's `dyn NavigationService`
([navigation](navigation.md): `add_navigation(..)`, or the app's own), registered before or after it, and clears on
every route change. Without one the app does not build, and the error names `add_navigation(..)`.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `selection` feature on, or
`rayx_services_selection::register(registry)` by hand.

## The contract

`SelectionService` (`SelectionServiceContract` in TS). An area is a `SelectionAreaId` (a number in TS); its
`SelectionStatus` holds the participants, the selected characters, whether it is collapsed (a caret) and whether a
selection is in progress (`has_selection()`: anything selected, or a caret placed). Only the active area publishes,
and every route change clears it.

Published state:

- `selection()` (`selection`): a `StateCell<SelectionState>`, the active area, its status and the revisions
  (`navigation_revision`, `active_area_revision`, `status_revision`), published only when it changes.

Commands (`async`, `Result<(), SelectionError>`; promises in TS):

- `activate(area, status)` (`activate`): makes `area` the area that publishes, with `status`.
- `publish_status(area, status)` (`publishStatus`): its status changed (dropped when `area` is not the active one).
- `deactivate(area)` (`deactivate`): clears the selection when `area` is the active one, so a stale area never
  clears a newer one's.
- `clear()` (`clear`): clears the active area and its status.

Rust helpers on `dyn SelectionService`:

- `active_area()`, `status()`, `navigation_revision()`, `active_area_revision()` and `status_revision()`: read the
  state synchronously.
- `subscribe(|area, status| ..)`: hears every change of the area or its status, until its subscription is dropped.

## Use

```rust
use rayx::app::services::selection::{SelectionAreaId, SelectionService, SelectionStatus};

let selection = services.required_service::<dyn SelectionService>()?;
let area = SelectionAreaId::from_raw(3);
let status = SelectionStatus { participant_count: 1, selected_character_count: 4, ..SelectionStatus::default() };
selection.activate(area, status).await?;       // this area publishes now
selection.publish_status(area, status).await?; // its status changed
selection.deactivate(area).await?;
```

RayX's commands take effect before they return; await them as any implementation's.

## On NSH

RayX's implementation holds no NSH state: it subscribes to the app's `dyn NavigationService` route changes and
clears the selection on each, counting them in `navigation_revision`. The components that own a selection publish
its summary through the service.

## TSX

`SelectionService`, resolved from the provider `start()` gives, wraps whichever `dyn SelectionService` is registered:

- `state()`: `{ active_area, status, navigation_revision, active_area_revision, status_revision }`;
- `activeArea()` (`null` when none is active) and `status()`;
- `onChange((state) => ..)`: every change, until the function it returns is called;
- `activate(area, status)`, `publishStatus(area, status)`, `deactivate(area)` and `clear()`: promises that resolve
  once the service took them and reject with its error.

```tsx
const selection = services.getRequiredService(SelectionService);
await selection.activate(3, {
  participant_count: 1, selected_character_count: 4, is_collapsed: false, is_selecting: false,
});
selection.activeArea(); // 3
const stop = selection.onChange((state) => show(state.status));
```

## Replace it

An app may register its own `dyn SelectionService` in place of `add_selection()` (it then needs no navigation
service); the class `SelectionService` wraps it.

```rust
builder.add_singleton_as::<dyn SelectionService>(Arc::new(MySelection::new()) as Arc<dyn SelectionService>)
```

In TS, a class implementing `SelectionServiceContract`, registered under `SelectionService`: its state a
`ContractState`, the commands `async` methods. Values cross as the JSON of the Rust types (snake_case fields).

```tsx
const empty = { participant_count: 0, selected_character_count: 0, is_collapsed: false, is_selecting: false };
const state = (area, status) => ({
  active_area: area, status, navigation_revision: 0, active_area_revision: 0, status_revision: 0,
});

class MySelection {
  selection = new ContractState(state(null, empty));
  async activate(activeArea, status) { this.selection.publish(state(activeArea, status)); }
  async publishStatus(activeArea, status) {
    if (this.selection.value.active_area === activeArea) this.selection.publish(state(activeArea, status));
  }
  async deactivate(activeArea) {
    if (this.selection.value.active_area === activeArea) this.selection.publish(state(null, empty));
  }
  async clear() { this.selection.publish(state(null, empty)); }
}

const services = await new RayXAppBuilder().addSingletonAs(SelectionService, () => new MySelection()).start();
```

For tests, `rayx_app::testing::FakeSelection` publishes the state a test gives it (`publish`) and records the
commands (`calls()`).
