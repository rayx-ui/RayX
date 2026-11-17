# Platform

The device and the user's settings, as a snapshot that changes while the app runs.

## Register

```rust
use rayx_services_platform::PlatformAppBuilderExt as _;

builder.add_platform()  // TS: builder.addPlatform()
```

`add_platform()` registers RayX's implementation (`rayx_services_platform`) as the app's `dyn PlatformInfoService`; the
app's start requires one, RayX's or the app's own.

## What it holds

`PlatformInfoService` (in `rayx_app::services::platform`) publishes a `PlatformSnapshot` as its state:

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
let platform = services.required_service::<dyn PlatformInfoService>()?;
let snapshot = platform.platform().get();
let subscription = platform.platform().subscribe(|snapshot| relayout(snapshot));
```

Only its implementation publishes; `rayx_app::testing::FakePlatform` publishes what a test gives it.

## On NSH

Every surface (window) has its own root provider, so a surface's service describes that surface. Starting the app
copies NSH's environment into it before the startup services initialize, so they see the device and the user's
settings; it keeps itself current from then on, in Rust and TSX apps alike.

The displays and the window come from NSH's `displays()` and `window_state()`, which only desktops report; the text
direction is CLDR's for the first preferred language. The platform's name, Catalyst, Apple's shortcuts, the keyboard's
animation, active descendants and a terminal's cell come from NSH's environment (`platform_name()`, `is_catalyst()`,
`apple_keys()`, `keyboard_duration()`, `reads_active_descendants()`, `cell_size()`), shaders and the colour picker
from `shaders_available()` and `can_pick_screen_color()`, and web views and speech from what NSH answers once the app
starts (`web_view_available()`, `web_evaluate_available()`, `web_input_available()`, `speech_available()`). NSH reports no high-contrast setting and gives Rust code no
user-writable directory: apps write through [storage](storage.md).

## TSX

`PlatformInfoService`, resolved from the provider `start()` gives:

```tsx
const platform = services.getRequiredService(PlatformInfoService);
const device = platform.current();
const stop = platform.onChange((snapshot) => relayout(snapshot)); // `stop()` ends it
```
