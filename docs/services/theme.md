# Theme

Which context each modifier of the active theme document shows (`accent`, `density`, `os`, the light/dark modifier),
and whether light/dark follows the color scheme shown or the app's choice, behind the `ThemeService` contract.

When the service is registered, light/dark changes through it. Density, OS and contrast are not followed from the
platform: an app picks them with `set_context`. Without the service, nothing picks: RayX's theme shows each
modifier's default, `light` for its `colorMode`.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::theme::ThemeService` (`rayx::app::services::theme`) |
| Crate | `rayx_services_theme` (`rayx::services::theme`) |
| Feature | `theme` |
| Rust | `add_theme(configure)` (`ThemeAppBuilderExt`) |
| TS | `addTheme(configure?)`, and the classes `ThemeService` and `ThemeOptions` |
| Requires | `dyn PlatformInfoService` (`add_platform()`) |

```rust
use rayx::prelude::*; // `ThemeAppBuilderExt`, with the `theme` feature

builder
    .add_platform()
    .add_theme(|options| {
        options.context("accent", "cyan");
    })
```

```tsx
new RayXAppBuilder()
  .addPlatform()
  .addTheme((options) => options.context("accent", "cyan"));
```

`add_theme` registers RayX's implementation as the app's `dyn ThemeService`, replacing any earlier registration of
it, as a startup service: before the app is ready it applies the start contexts and the color mode to the surface's
theme. It follows the color scheme of the app's [platform service](platform.md), which it requires and does not
register: building without one fails, naming it. In TS, `addTheme` is there once the crate's set is registered
(`#[rayx::native_module]` with the feature on, or `rayx_services_theme::register(registry)`). `rayx new`'s apps
register it.

## Configure

`ThemeOptions` binds from the `RayX:Services:Theme` section (`THEME_CONFIG_PATH`), then the closure changes it (in TS,
the callback's `colorMode(choice)`, `colorModeModifier(modifier, light, dark)` and `context(modifier, context)`, after
configuration's):

```json
{
  "RayX": {
    "Services": {
      "Theme": {
        "colorMode": { "modifier": "colorMode", "light": "light", "dark": "dark", "choice": "System" },
        "contexts": { "accent": "cyan", "density": "touch" }
      }
    }
  }
}
```

- `colorMode` names the theme's light/dark modifier and its two contexts (by default `colorMode`, `light` and
  `dark`, RayX's theme's) and the choice it starts with: `System` (the default), `Light` or `Dark`.
- `contexts` are the picks to apply at start, by modifier. A pick the active document lacks is logged once at startup
  and skipped.
- Startup refuses, naming the entry: an empty name, the same context for light and dark, and a `contexts` entry for the
  light/dark modifier (`colorMode.choice` sets it).
- Keys match ignoring ASCII case, `_` and `-`; the modifier and context names in `contexts` are the document's, as
  written.

## The contract

`ThemeService` (TS: `ThemeServiceContract`) is NSH-free and implementable in Rust or TS.

Published state:

- `theme()` (TS `theme`): a `ThemeSnapshot`: for each of the active document's modifiers a `ThemeModifier`
  (`name`, `contexts`, `default`, `selected`, the context it shows), the light/dark choice (`color_mode`, a
  `ColorModeChoice`: `System`, `Light` or `Dark`) and a `revision` that increases by one with every snapshot.

Commands:

- `set_context(modifier, context)` (TS `setContext`): shows `context` for `modifier`; whether it differs from the
  snapshot's `selected` at the call. For the light/dark modifier it takes only its light and dark contexts and sets
  the choice to `Light` or `Dark`. An unknown modifier or context is refused (`ThemeError::UnknownModifier`,
  `UnknownContext`) and changes nothing.
- `set_color_mode(choice)` (TS `setColorMode`): follows the color scheme shown again (`System`) or shows one context
  (`Light`, `Dark`); whether the choice changed.

`ThemeError::Failed` carries what a call's implementation threw, or that the engine is gone. Rust helpers (on
`dyn ThemeService`), synchronous functions of the snapshot whatever the implementation: `snapshot()`,
`modifier(name)`, `selected(name)` and `color_mode()`.

## Use

```rust
let theme = services.required_service::<dyn ThemeService>()?;
let accent = theme.selected("accent");
let subscription = theme.theme().subscribe(|snapshot| redraw(snapshot));

theme.set_context("density".into(), "touch".into()).await?;
theme.set_color_mode(ColorModeChoice::Dark).await?;
```

Components need none of this: they read RayX's roles, which follow the theme's modifiers by themselves.

## On NSH

Each surface has its own service, with its own picks, and NSH owns the selection:

- **The first frame.** `add_theme` (TS `addTheme`) picks at once, where it runs (while the root is built, or before
  `mount`), from `configure`'s options: the start contexts and the light/dark context of the choice (with `System`,
  `dark` exactly when NSH shows dark). The app's first frame, a splash included, shows them. Startup then reconciles
  that pick with the options bound from configuration and with the platform service.
- **Picks from any thread.** A command validates the pick against the snapshot and sends it to the engine thread,
  which applies it with NSH's `set_modifier`; it never blocks the engine thread.
- **The snapshot follows NSH.** The service watches NSH's `modifiers()` and publishes from them, so a `theme()`
  listener runs once NSH already shows a pick. A change of document (`set_theme`) shows in the snapshot: a modifier the
  new document lacks leaves it, and the picks of those that remain stay.
- **`System` follows what NSH shows.** The light/dark modifier shows its light or dark context as the platform
  service's `color_scheme` gives it: the app's appearance when the app forced one (`nsh.json`'s `appearance`), else
  the system's. Under `System` the service never sets NSH's appearance.
- **`Light` and `Dark` set NSH's appearance** (`set_color_scheme`) to match, so `dark:` variants and native chrome
  agree with the tokens; returning to `System` gives NSH the system's appearance back.
- **A pick made around the service is adopted.** An app's own `set_modifier` of the light/dark modifier to its light
  or dark context makes the choice `Light` or `Dark`, with NSH's appearance to match. Another context of that modifier
  shows as `selected` and keeps the choice.
- **A document without the light/dark modifier** changes no token: the snapshot shows no such modifier, and the choice
  is kept for a later document that has it.

Outside an engine (a unit test) nothing is picked: the commands validate and set the choice, and the snapshot lists no
modifiers.

## TSX

`ThemeService`, resolved from the provider `start()` gives, wraps whichever `dyn ThemeService` is registered:

- `current()`: the snapshot (its JSON, snake_case keys: a `ThemeSnapshot` in TS);
- `setContext(modifier, context)` and `setColorMode(choice)`: promises of whether the pick changed, rejecting with the
  error's message;
- `onChange(listener)`: hears every snapshot; it returns a function that stops it.

```tsx
// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function pickTheme(services: ServiceProvider) {
  const theme = services.getRequiredService(ThemeService);
  const stop = theme.onChange((snapshot) => console.log(snapshot.color_mode));
  await theme.setContext("accent", "sky");
  await theme.setColorMode("Dark");
}
```

## Replace it

Register the app's own `dyn ThemeService` instead of calling `add_theme` (it then needs no platform service unless it
uses one):

```rust
builder.add_singleton_as::<dyn ThemeService>(Arc::new(MyTheme::new()) as Arc<dyn ThemeService>)
```

In TS, a class implementing `ThemeServiceContract`, registered under the class `ThemeService`
([contracts](../tsx.md#contracts)):

```tsx
class MyTheme implements ThemeServiceContract {
  theme = new ContractState<ThemeSnapshot>({ modifiers: [], color_mode: "System", revision: 0 });
  async setContext(modifier: string, context: string) { /* … */ return true; }
  async setColorMode(choice: ColorModeChoice) { /* … */ return false; }
}

new RayXAppBuilder().addSingletonAs(ThemeService, () => new MyTheme());
```

Rust's helpers and the class then read its published state (a field left out is its default); what it throws reaches
Rust as `ThemeError::Failed`.

`rayx_app::testing::FakeTheme` is a fake of the contract, publishing the snapshot a test gives it, recording the
commands (`ThemeCall`) and refusing a modifier or context its snapshot lacks.
