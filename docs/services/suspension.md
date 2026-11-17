# Suspension

The app's lifecycle, focus and visibility, a desktop's window and the system's wakes from sleep, and quitting the app.

## Register

```rust
builder.add_suspension()
```

`add_suspension()` (from `rayx_services_suspension`; `builder.addSuspension()` in TS) registers RayX's implementation
as the app's `dyn SuspensionService`: it follows NSH's app state from its startup on and quits through NSH. An app may
register its own `dyn SuspensionService` instead.

## What it holds

A `SuspensionSnapshot`:

- the lifecycle: not started, running, shutting down, shut down;
- the activation: whether the app has focus;
- the visibility: foreground or background;
- on a desktop, the app's window (`window`): whether it has the focus, is minimized and is on the screen;
- `wakes`: how many times the system woke from sleep since the app started.

## Use

```rust
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
  hears every event, `suspension()` the state.
- A change is a change of lifecycle, activation, visibility, window or wakes; an event that only moves the timestamp
  is not one.
- `Woke`: the system woke from sleep while the app ran (a desktop). The app's state may be as it was (a laptop's lid
  closed and opened with the app in front), but network connections and timers did not live through the sleep: check
  or open them again. On iOS and Android a sleeping device takes the app to the background (`Backgrounded`).
- `Destroying`, then `Destroyed`: the app quits. Save what must not be lost on `Destroying`, synchronously and
  quickly: the platform waits for the app only briefly.
- Timestamps are optional and come from the system clock where the target has one.

## Asking before quitting

```rust
let asking = suspension.ask_before_quit(move || confirm_quit.set(true));
// ... once the user confirmed, or the work finished:
suspension.quit();
```

While an `ask_before_quit` subscription lives, the user's requests to quit (the app's Quit item, Command-Q, Ctrl+Q)
no longer quit: each is a `QuitRequested` event and goes to the listeners, and the app calls `quit()` when it is ready.
Dropping the last subscription makes them quit again. The system ending the app (logging out, shutting down) never
asks: the app hears `Destroying`. `quit()` quits whether or not the app asks. Both are a desktop's (macOS, Windows,
Linux); elsewhere the system decides when an app ends, and they do nothing.

## On NSH

RayX's implementation publishes, from its startup on, in Rust and TSX apps alike:

- NSH's `lifecycle()`: NSH folds the platforms' own callbacks into one app state, so an event's reason is
  `AppStateChanged` (or the host's start);
- on a desktop, the app's window (`WindowChanged`, for `WindowStateChanged`) from NSH's `window_state()`;
- the system's wakes (`Woke`, for `SystemWoke`) from NSH's `wake_count()`;
- the user's requests to quit while the app asks (`QuitRequested`, for `UserAskedToQuit`) from NSH's
  `on_quit_request`;
- the app's quit from NSH's `on_app_quit`: `Destroying` (`HostStopping`), then the [storage](storage.md) writes what
  still waits for the engine, then `Destroyed` (`HostStopped`).

`quit()` quits with NSH's `quit()` and `ask_before_quit(..)` asks with `on_quit_request`; both work from any
thread. The [platform service](platform.md) holds the platform's state.

## TSX

`SuspensionService`, resolved from the provider `start()` gives: `lifecycle()` returns the snapshot,
`onLifecycle(listener)` hears every event (`{ kind, reason, timestamp_unix_millis, window }`, with `kind` as Rust names
it: `"Woke"`, `"QuitRequested"`, `"Destroying"`, ...), `askBeforeQuit(listener)` asks before quitting, and `quit()`
quits. Each listener lasts until the function it returns is called, or the object is disposed.

```tsx
const suspension = services.getRequiredService(SuspensionService);
suspension.onLifecycle((event) => {
  if (event.kind === "Woke") reconnect();
});
const stopAsking = suspension.askBeforeQuit(() => (hasUnsavedWork() ? confirmQuit.set(true) : suspension.quit()));
```
