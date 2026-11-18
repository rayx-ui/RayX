# Platform

The device and the user's settings, as a snapshot that changes while the app runs.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::platform::PlatformInfoService` (`rayx::app::services::platform`) |
| Crate | `rayx_services_platform` (`rayx::services::platform`) |
| Feature | `platform` (default) |
| Rust | `add_platform()` (`PlatformAppBuilderExt`) |
| TS | `addPlatform()`, and the class `PlatformInfoService` |
| Requires | nothing |

```rust
use rayx::prelude::*; // PlatformAppBuilderExt, with the `platform` feature

let builder = RayXAppBuilder::new().add_platform();
// TS: new RayXAppBuilder().addPlatform()
```

`add_platform()` registers RayX's implementation as the app's `dyn PlatformInfoService`, replacing any earlier
registration of it: a startup service (keyed `dyn PlatformInfoService`) that publishes NSH's environment when it
initializes and keeps it current from then on. The app's start requires a `dyn PlatformInfoService`, RayX's or the
app's own; [application configuration](application-configuration.md) and [i18n](i18n.md) require one too.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `platform` feature on, or
`rayx_services_platform::register(registry)` by hand.

## The contract

`PlatformInfoService` (`PlatformInfoServiceContract` in TS) is read from any thread.

Published state:

- `platform()` (`platform`): a `StateCell<PlatformSnapshot>`, the device and the user's settings now and each change.

Rust helpers:

- `on_ready_when_platform(predicate, hook)` (`rayx_app::services::platform::PlatformReadyAppBuilderExt`, on the app
  builder): runs `hook` once the app is ready when `predicate` holds for the snapshot then; without a platform service
  it does not run ([Startup](../startup.md)).
- On the snapshot: `primary_display()`, `display(id)`, `window_display()`, `orientation()`, and
  `form_factor.is_handheld()`.

A `PlatformSnapshot` holds:

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
  mode, focus, minimized, visible, bounds and display).

A TV (an Android TV, a TV's browser, Tizen) has the `Tv` form factor: no pointer, an on-screen keyboard and a remote.

## Use

```rust
use rayx::app::services::platform::PlatformInfoService;

let platform = services.required_service::<dyn PlatformInfoService>()?;
let snapshot = platform.platform().get(); // Arc<PlatformSnapshot>
let subscription = platform.platform().subscribe(|snapshot| relayout(snapshot)); // until dropped
```

A listener hears each snapshot published after it subscribed, not the one now. In a Rust component,
`use_state_cell(platform.platform())` gives the snapshot as an NSH signal.

## On NSH

Every surface (window) has its own root provider, so a surface's service describes that surface. RayX's
implementation publishes NSH's environment as it initializes, so a startup service that depends on
`dyn PlatformInfoService` sees the device and the user's settings, and follows it from then on, in Rust and TSX apps
alike.

The displays and the window come from NSH's `displays()` and `window_state()`, which only desktops report; the text
direction is CLDR's for the first preferred language. The platform's name, Catalyst, Apple's shortcuts, the keyboard's
animation, active descendants and a terminal's cell come from NSH's environment (`platform_name()`, `is_catalyst()`,
`apple_keys()`, `keyboard_duration()`, `reads_active_descendants()`, `cell_size()`), shaders and the colour picker
from `shaders_available()` and `can_pick_screen_color()`, and web views and speech from what NSH answers once the
service initialized (`web_view_available()`, `web_evaluate_available()`, `web_input_available()`,
`speech_available()`). NSH reports no high-contrast setting and gives Rust code no user-writable directory: apps write
through [storage](storage.md).

`rayx_services_platform::platform_snapshot()` reads NSH's environment as a snapshot now, on the engine thread.

## TSX

`PlatformInfoService`, resolved from the provider `start()` gives, wraps whichever `dyn PlatformInfoService` is
registered:

- `current()`: the snapshot now, the JSON of `PlatformSnapshot` (snake_case fields);
- `onChange(listener)`: calls `listener` with the snapshot after every change, until the function it returns is
  called (or the reactive owner of the call goes), or the object is disposed.

```tsx
const platform = services.getRequiredService(PlatformInfoService);
const device = platform.current();
const stop = platform.onChange((snapshot) => relayout(snapshot)); // `stop()` ends it
```

## Replace it

An app may describe the platform itself (a host outside NSH, a test) by registering its own `dyn PlatformInfoService`
in place of `add_platform()`. RayX's [application configuration](application-configuration.md) and [i18n](i18n.md)
then follow it, the host starts with it, and the class `PlatformInfoService` wraps it.

```rust
builder.add_singleton_as::<dyn PlatformInfoService>(
    Arc::new(MyPlatform::new()) as Arc<dyn PlatformInfoService>
)
```

One that must initialize at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_platform()` does: the trait and a startup service registered as it, so the services that depend on that key wait for it
(`builder.add_startup_singleton_for::<dyn PlatformInfoService, _>(Arc::new(MyPlatform::new()))`, `MyPlatform` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_platform()`, the app's own replaces it completely: RayX's never starts.

In TS, a class implementing `PlatformInfoServiceContract`, registered under `PlatformInfoService`: `platform` a
`ContractState` holding a whole snapshot (the JSON of `PlatformSnapshot`).

```tsx
class MyPlatform {
  platform = new ContractState(snapshot); // a whole PlatformSnapshot, as JSON
  isInitialized() { return this.platform.value.platform_name === "mine"; }
  async initialize() {
    this.platform.publish({ ...this.platform.value, platform_name: "mine", color_scheme: "Dark" });
  }
}

new RayXAppBuilder()
  .addStartupSingletonAs(PlatformInfoService, () => new MyPlatform())
  .addSuspension()
  .addAssets()
  .addFonts()
  .start()
  .then((services) => { /* RayX's PlatformInfoService class wraps MyPlatform */ });
```

`addSingletonAs` registers one that needs no initialization. For tests, `rayx_app::testing::FakePlatform` publishes
the snapshot the test gives it (`publish`, `modify`).
