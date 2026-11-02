# Platform

The device and the user's settings, as a snapshot that changes while the app runs.

## Register

```rust
builder.add_platform()
```

## What it holds

`PlatformInfoService` holds a `PlatformSnapshot`:

- the OS family and version, form factor, desktop behaviour and input capabilities;
- the viewport, scale, safe area and keyboard height;
- the colour scheme, font scale, reduced motion and screen reader;
- the locale: languages, time zone, hour cycle, first weekday, measurement system and temperature unit.

## Use

```rust
let platform = services.required_service::<PlatformInfoService>()?;
let snapshot = platform.current();
let subscription = platform.subscribe(|event| relayout(event)); // a PlatformChangedEvent
```

`update(snapshot)` and `modify(|snapshot| ..)` change it and notify listeners.

## On NSH

Every surface (window) has its own root provider, so a surface's service describes that surface. `use_platform_sync()`
in a Rust root component, and `AppServices.load()` in a TSX app, copy NSH's environment into it.

NSH does not report the display topology (several monitors) or high contrast, and does not expose a user-writable
directory: apps write through [storage](storage.md).

## TSX

```tsx
const platform = Platform.of(app);
const device = platform.current();
platform.onChange((snapshot) => relayout(snapshot));
```
