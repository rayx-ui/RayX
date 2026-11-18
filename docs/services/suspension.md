# Suspension

The app's lifecycle, focus and visibility, a desktop's window and the system's wakes from sleep, and quitting the app.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::suspension::SuspensionService` (`rayx::app::services::suspension`) |
| Crate | `rayx_services_suspension` (`rayx::services::suspension`) |
| Feature | `suspension` (default) |
| Rust | `add_suspension()` (`SuspensionAppBuilderExt`) |
| TS | `addSuspension()`, and the class `SuspensionService` |
| Requires | nothing |

```rust
use rayx::prelude::*; // SuspensionAppBuilderExt, with the `suspension` feature

let builder = RayXAppBuilder::new().add_suspension();
// TS: new RayXAppBuilder().addSuspension()
```

`add_suspension()` registers RayX's implementation as the app's `dyn SuspensionService`, replacing any earlier
registration of it: a startup service (keyed `dyn SuspensionService`) that follows NSH's app state from its
initialization on and quits through NSH. The app's start requires a `dyn SuspensionService`, RayX's or the app's own.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `suspension` feature on, or
`rayx_services_suspension::register(registry)` by hand.

## The contract

`SuspensionService` (`SuspensionServiceContract` in TS) is called from any thread.

Published state:

- `suspension()` (`suspension`): a `StateCell<SuspensionSnapshot>`: the lifecycle (not started, running, shutting
  down, shut down), the activation (focus), the visibility (foreground, background), on a desktop the app's window
  (`window`: focused, minimized, visible), `wakes` (how many times the system woke from sleep since the app started)
  and `asks_before_quit`.

Published events:

- `events()` (`events`): an `EventStream<SuspensionEvent>`, each event (`kind`, `reason`, `timestamp_unix_millis`,
  `window`): `Started`, `Activated`, `Deactivated`, `Backgrounded`, `Foregrounded`, `Destroying`, `Destroyed`,
  `WindowChanged`, `Woke`, `QuitRequested`.

Commands (`async`, `Result<(), SuspensionError>`; promises in TS):

- `quit()` (`quit`): quits the app, whether or not it asks first.
- `ask_before_quit(ask)` (`askBeforeQuit`): while `ask` holds, the user's requests to quit come to the app as
  `QuitRequested` events instead of quitting it.

Rust helpers:

- `on_quit_request(listener)` on `Arc<dyn SuspensionService>`: asks before quitting and calls `listener` on each
  request, until the `ChangeSubscription` it returns is dropped, which stops asking.
- On the snapshot: `is_running()`, `is_active()`, `is_foreground()`, `is_backgrounded()`, `same_state(other)` and
  `apply(event)`, the state an event leads to.

## Use

```rust
use rayx::app::services::suspension::{SuspensionEventKind, SuspensionService};

let suspension = services.required_service::<dyn SuspensionService>()?;
let state = suspension.suspension().get();
let changes = suspension.suspension().subscribe(|state| pause_when_hidden(state));
let events = suspension.events().subscribe(|event| match event.kind {
    SuspensionEventKind::Woke => reconnect(),
    SuspensionEventKind::Destroying => save_draft(),
    _ => {}
});
```

- Its implementation publishes `SuspensionEvent`s and the state they lead to (`SuspensionSnapshot::apply`); `events()`
  hears every event, `suspension()` the state. A subscription lasts until it is dropped.
- A change is a change of lifecycle, activation, visibility, window, wakes or asking before quitting; an event that
  only moves the timestamp is not one.
- `Woke`: the system woke from sleep while the app ran (a desktop). The app's state may be as it was (a laptop's lid
  closed and opened with the app in front), but network connections and timers did not live through the sleep: check
  or open them again. On iOS and Android a sleeping device takes the app to the background (`Backgrounded`).
- `Destroying`, then `Destroyed`: the app quits. Save what must not be lost on `Destroying`, quickly: the platform
  waits for the app only briefly. On NSH a save on the engine thread, where the event is heard, is done at once; one
  another thread makes (a thread the listener handed it to) completes when its calls reach the engine thread through
  the surface's engine runner (RayX's storage, a TS contract) before the engine stops: the host runs them after the
  quit listeners, until none arrived, ran or ended for 50 ms, 150 ms at most. A call that reaches the engine later
  fails with `EngineGone`.
- Timestamps are optional and come from the system clock where the target has one.

### Asking before quitting

```rust
let asking = suspension.on_quit_request(move || confirm_quit()); // a ChangeSubscription
// ... once the user confirmed, or the work finished:
suspension.quit().await?;
drop(asking); // requests quit again
```

While the app asks (`on_quit_request`, or `ask_before_quit(true).await?`), the user's requests to quit (the app's
Quit item, Command-Q, Ctrl+Q) no longer quit: each is a `QuitRequested` event, and the app calls `quit()` when it is
ready. Dropping the subscription (or `ask_before_quit(false)`) makes them quit again; `on_quit_request` has one owner
at a time, and dropping any subscription stops asking. The system ending the app (logging out, shutting down) never
asks: the app hears `Destroying`. `quit()` quits whether or not the app asks. Both are a desktop's (macOS, Windows,
Linux); elsewhere the system decides when an app ends, and they do nothing.

## On NSH

RayX's implementation publishes, from its initialization on, in Rust and TSX apps alike:

- NSH's `lifecycle()`: NSH folds the platforms' own callbacks into one app state, so an event's reason is
  `AppStateChanged` (or the host's start, `HostStarted`);
- on a desktop, the app's window (`WindowChanged`, for `WindowStateChanged`) from NSH's `window_state()`;
- the system's wakes (`Woke`, for `SystemWoke`) from NSH's `wake_count()`;
- the user's requests to quit while the app asks (`QuitRequested`, for `UserAskedToQuit`) from NSH's
  `on_quit_request`;
- the app's quit from NSH's `on_app_quit`: `Destroying` (`HostStopping`), then `Destroyed` (`HostStopped`); the host
  then runs what other threads queue for the engine (a [storage](storage.md) save made as they heard `Destroying`)
  before the engine stops.

`quit()` quits with NSH's `quit()` and `ask_before_quit(..)` asks with `on_quit_request`; both work from any thread
(through the engine thread's task, and kept until the service initialized). The [platform service](platform.md) holds
the platform's state.

## TSX

`SuspensionService`, resolved from the provider `start()` gives, wraps whichever `dyn SuspensionService` is
registered:

- `lifecycle()`: the snapshot now, the JSON of `SuspensionSnapshot`;
- `onLifecycle(listener)`: hears every event (`{ kind, reason, timestamp_unix_millis, window }`, with `kind` as Rust
  names it: `"Woke"`, `"QuitRequested"`, `"Destroying"`, ...);
- `askBeforeQuit(listener)`: the user's requests to quit call `listener` instead of quitting; the app asks while one
  of the object's `askBeforeQuit` listeners lives;
- `quit()`: quits (it returns nothing).

Each listener lasts until the function it returns is called (or the reactive owner of the call goes), or the object
is disposed.

```tsx
import { signal } from "nsh";

declare function reconnect(): void; // the app's own
declare function hasUnsavedWork(): boolean;
const confirmQuit = signal(false); // shows the app's "Quit without saving?" prompt

// With the provider `start()` gives:
function followLifecycle(services: ServiceProvider) {
  const suspension = services.getRequiredService(SuspensionService);
  suspension.onLifecycle((event) => {
    if (event.kind === "Woke") reconnect();
  });
  const stopAsking = suspension.askBeforeQuit(() => (hasUnsavedWork() ? confirmQuit.set(true) : suspension.quit()));
}
```

## Replace it

An app may follow its lifecycle itself by registering its own `dyn SuspensionService` in place of `add_suspension()`.
The host starts with it, what an app saves in RayX's [storage](storage.md) as it hears `Destroying` reaches the
device (NSH writes every change itself; when it publishes `Destroying` from NSH's quit listeners, the host runs the
calls other threads queue for the engine after them, as above), and the class `SuspensionService` wraps it.

```rust
builder.add_singleton_as::<dyn SuspensionService>(
    Arc::new(MySuspension::new()) as Arc<dyn SuspensionService>
)
```

One that must initialize at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_suspension()` does: the trait and a startup service registered as it
(`builder.add_startup_singleton_for::<dyn SuspensionService, _>(Arc::new(MySuspension::new()))`, `MySuspension` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_suspension()`, the app's own replaces it completely: RayX's never starts.

In TS, a class implementing `SuspensionServiceContract`, registered under `SuspensionService`: `suspension` a
`ContractState` holding a whole snapshot, `events` a `ContractEvents`, `quit()` and `askBeforeQuit(ask)` `async`
methods.

```tsx
class MySuspension implements SuspensionServiceContract {
  suspension = new ContractState<SuspensionSnapshot>({
    lifecycle: "NotStarted", activation: "Unknown", visibility: "Unknown", updated_at_unix_millis: null,
    last_reason: null, window: null, wakes: 0, asks_before_quit: false,
  });
  events = new ContractEvents<SuspensionEvent>();
  isInitialized() { return this.suspension.value.lifecycle === "Running"; }
  async initialize() {
    this.suspension.publish({ ...this.suspension.value, lifecycle: "Running", visibility: "Foreground" });
    this.events.publish({ kind: "Started", reason: "HostStarted", timestamp_unix_millis: null, window: null });
  }
  async quit() { /* quit */ }
  async askBeforeQuit(ask: boolean) { this.suspension.publish({ ...this.suspension.value, asks_before_quit: ask }); }
}

new RayXAppBuilder()
  .addPlatform()
  .addStartupSingletonAs(SuspensionService, () => new MySuspension())
  .addAssets()
  .addFonts()
  .start()
  .then((services) => { /* RayX's SuspensionService class wraps MySuspension */ });
```

For tests, `rayx_app::testing::FakeSuspension` publishes the events the test gives it (`emit`, `start`) and counts the
quits (`quits`).
