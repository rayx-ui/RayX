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
- the locale: languages, time zone, hour cycle, first weekday, measurement system, temperature unit and the direction
  of the first language's script;
- on a desktop, the displays (`displays`: bounds, visible bounds, scale, primary) and the app's window (`window`:
  mode, focus, minimized, visible, bounds and display); `primary_display()` and `window_display()` find theirs.

## Use

```rust
let platform = services.required_service::<PlatformInfoService>()?;
let snapshot = platform.current();
let subscription = platform.subscribe(|event| relayout(event)); // a PlatformChangedEvent
```

`update(snapshot)` and `modify(|snapshot| ..)` change it and notify listeners.

## On NSH

Every surface (window) has its own root provider, so a surface's service describes that surface. Starting the app
copies NSH's environment into it before the startup services initialize, so they see the device and the user's
settings; `use_platform_sync()` in a Rust root component, and `AppServices.load()` in a TSX app, keep it current from
then on.

The displays and the window come from NSH's `displays()` and `window_state()`, which only desktops report; the text
direction is CLDR's for the first preferred language. NSH reports no high-contrast setting and gives Rust code no
user-writable directory: apps write through [storage](storage.md).

## TSX

```tsx
const platform = Platform.of(app);
const device = platform.current();
platform.onChange((snapshot) => relayout(snapshot));
```
