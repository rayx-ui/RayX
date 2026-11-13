# Suspension

The app's lifecycle, focus and visibility, a desktop's window and the system's wakes from sleep, and quitting the app.

## Register

```rust
builder.add_suspension()
```

`add_suspension` quits through the app's `dyn SuspensionHost` when one is registered (`rayx_app` registers NSH's;
`with_suspension_host(host)` registers another).

## What it holds

A `SuspensionSnapshot`:

- the lifecycle: not started, running, shutting down, shut down;
- the activation: whether the app has focus;
- the visibility: foreground or background;
- on a desktop, the app's window (`window`): whether it has the focus, is minimized and is on the screen;
- `wakes`: how many times the system woke from sleep since the app started.

## Use

```rust
let suspension = services.required_service::<SuspensionService>()?;
let state = suspension.current();
let changes = suspension.subscribe_changed(|change| pause_when_hidden(change));
let events = suspension.subscribe_events(|event| match event.kind {
    SuspensionEventKind::Woke => reconnect(),
    SuspensionEventKind::Destroying => save_draft(),
    _ => {}
});
```

- The UI layer moves it with `SuspensionEvent`s (`publish(event)`). `subscribe_events` hears every event,
  `subscribe_changed` only changes of state.
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

`rayx_app` publishes, from `use_platform_sync()` in a Rust root or `AppServices.load()` in a TSX app:

- NSH's `lifecycle()`: NSH folds the platforms' own callbacks into one app state, so an event's reason is
  `AppStateChanged` (or the host's start);
- on a desktop, the app's window (`WindowChanged`, for `WindowStateChanged`) from NSH's `window_state()`;
- the system's wakes (`Woke`, for `SystemWoke`) from NSH's `wake_count()`;
- the user's requests to quit while the app asks (`QuitRequested`, for `UserAskedToQuit`) from NSH's
  `on_quit_request`;
- the app's quit from NSH's `on_app_quit`: `Destroying` (`HostStopping`), then the [storage](storage.md) writes what
  still waits for the engine, then `Destroyed` (`HostStopped`).

`NshSuspensionHost` quits with NSH's `quit()` and asks before quitting with `on_quit_request`; both work from any
thread. The [platform service](platform.md) holds the platform's state.

## TSX

`Platform.of(app)`: `lifecycle()` returns the snapshot, `onLifecycle(listener)` hears every event (`{ kind, reason,
timestamp_unix_millis, window }`, with `kind` as Rust names it: `"Woke"`, `"QuitRequested"`, `"Destroying"`, ...),
`askBeforeQuit(listener)` asks before quitting until the object is disposed, and `quit()` quits.

```tsx
const platform = Platform.of(app);
platform.onLifecycle((event) => {
  if (event.kind === "Woke") reconnect();
});
platform.askBeforeQuit(() => (hasUnsavedWork() ? confirmQuit.set(true) : platform.quit()));
```
