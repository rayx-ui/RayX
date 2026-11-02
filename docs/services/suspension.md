# Suspension

The app's lifecycle, focus and visibility.

## Register

```rust
builder.add_suspension()
```

## What it holds

A `SuspensionSnapshot`:

- the lifecycle: not started, running, shutting down;
- the activation: whether the app has focus;
- the visibility: foreground or background.

## Use

```rust
let suspension = services.required_service::<SuspensionService>()?;
let state = suspension.current();
let changes = suspension.subscribe_changed(|change| pause_when_hidden(change));
let events = suspension.subscribe_events(|event| log(event));
```

- The UI layer moves it with `SuspensionEvent`s (`publish(event)`). `subscribe_events` hears every event,
  `subscribe_changed` only changes of state.
- A change is a change of lifecycle, activation or visibility; an event that only moves the timestamp is not one.
- Timestamps are optional and come from the system clock where the target has one.

## On NSH

`rayx_app` publishes NSH's `app_state()`, from `use_platform_sync()` in a Rust root or `AppServices.load()` in a TSX
app. NSH folds the platforms' own callbacks into one app state, so an event's reason is `AppStateChanged` (or the host's
start and stop). The [platform service](platform.md) holds the platform's state.

## TSX

`Platform.of(app).lifecycle()` returns the snapshot; `onChange` hears it change.
