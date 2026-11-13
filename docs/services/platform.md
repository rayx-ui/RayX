# Platform

The device and the user's settings, as a snapshot that changes while the app runs.

## Register

```rust
builder.add_platform()
```

## What it holds

`PlatformInfoService` holds a `PlatformSnapshot`:

- the OS family and version, the platform's name (`platform_name`: the OS's on NSH's own platforms, a platform's
  built outside NSH on its engine, `"miniprogram"`), Mac Catalyst (`catalyst`), form factor and desktop behaviour;
- the input capabilities (`input`: pointer, keyboard, touch, and a TV's `remote`, which moves the focus with no
  pointer) and whether shortcuts follow Apple's conventions (`apple_keyboard`);
- the viewport, scale, safe area, and the on-screen keyboard's height and animation (`keyboard_animation_millis`);
- the colour scheme, font scale, reduced motion, screen reader, and whether screen readers follow a focused control's
  active descendant (`reads_active_descendants`);
- the locale: languages, time zone, hour cycle, first weekday, measurement system, temperature unit and the direction
  of the first language's script;
- in a terminal, the size of one cell of its grid (`terminal_cell`);
- what the platform can do (`features`): shader views, the screen colour picker, web views (showing them, running
  scripts in them, trusted input), speech recognition; `false` until the platform tells;
- on a desktop, the displays (`displays`: bounds, visible bounds, scale, primary) and the app's window (`window`:
  mode, focus, minimized, visible, bounds and display); `primary_display()` and `window_display()` find theirs.

A TV (an Android TV, a TV's browser, Tizen) has the `Tv` form factor: no pointer, an on-screen keyboard and a remote.

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
direction is CLDR's for the first preferred language. The platform's name, Catalyst, Apple's shortcuts, the keyboard's
animation, active descendants and a terminal's cell come from NSH's environment (`platform_name()`, `is_catalyst()`,
`apple_keys()`, `keyboard_duration()`, `reads_active_descendants()`, `cell_size()`), shaders and the colour picker
from `shaders_available()` and `can_pick_screen_color()`, and web views and speech from what NSH answers once the app
starts (`web_view_available()`, `web_evaluate_available()`, `web_input_available()`, `speech_available()`). NSH reports no high-contrast setting and gives Rust code no
user-writable directory: apps write through [storage](storage.md).

## TSX

```tsx
const platform = Platform.of(app);
const device = platform.current();
platform.onChange((snapshot) => relayout(snapshot));
```
