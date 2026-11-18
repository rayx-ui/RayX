# Application configuration

The app's own settings: its name and description, motion, scrolling, tooltips, hover cards, notifications and layout
direction, behind the `ApplicationConfigurationService` contract.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::application_configuration::ApplicationConfigurationService` (`rayx::app::services::application_configuration`) |
| Crate | `rayx_services_application_configuration` (`rayx::services::application_configuration`) |
| Feature | `application_configuration` |
| Rust | `add_application_configuration(configure)` (`ApplicationConfigurationAppBuilderExt`) |
| TS | `addApplicationConfiguration(configure?)`, and the classes `ApplicationConfigurationService` and `ApplicationConfigurationOptions` |
| Requires | `dyn PlatformInfoService` (`add_platform()`) |

```rust
use rayx::prelude::*; // `ApplicationConfigurationAppBuilderExt`, with the `application_configuration` feature

builder
    .add_app_settings("my-app")
    .add_platform()
    .add_application_configuration(|settings| settings.notifications.max_items = 3)
```

```tsx
new RayXAppBuilder()
  .addPlatform()
  .addApplicationConfiguration((options) => options.name("My App").merge({ notifications: { maxItems: 3 } }));
```

`add_application_configuration` registers RayX's implementation as the app's `dyn ApplicationConfigurationService`,
replacing any earlier registration of it. It is a plain singleton, not a startup service, but its settings are
validated at startup: invalid settings fail it. It follows the motion setting of the app's
[platform service](platform.md), which it requires and does not register: building without one fails, naming it. In
TS, `addApplicationConfiguration` is there once the crate's set is registered (`#[rayx::native_module]` with the
feature on, or `rayx_services_application_configuration::register(registry)`).

## Configure

`ApplicationConfiguration` binds from the `RayX:Application` section (`APPLICATION_CONFIGURATION_PATH`), then the
closure changes it (in TS, the callback's `name(..)`, `description(..)` and `merge(settings)`, after configuration's):

```json
{
  "RayX": {
    "Application": {
      "name": "My App",
      "motion": { "policy": "System" },
      "scrolling": { "programmatic": "Smooth", "smoothDurationMs": 200 },
      "tooltips": { "showDelayMs": 250 },
      "notifications": { "placement": "bottom_right", "maxItems": 3 },
      "layout": { "direction": "rtl" }
    }
  }
}
```

- The name and description apply at startup only.
- The name must not be empty, and `notifications.autoHideMs`, `notifications.maxItems` and
  `scrolling.smoothDurationMs` must be greater than zero.
- Keys match ignoring ASCII case, `_` and `-`; unknown keys are ignored (as .NET does), so a shared
  `appsettings.json` may carry keys of other parts.
- When the configuration reloads, the service takes the new settings (a startup-only or invalid change keeps them as
  they are).

## The contract

`ApplicationConfigurationService` (TS: `ApplicationConfigurationServiceContract`) is NSH-free and implementable in
Rust or TS.

Published state:

- `configuration()` (TS `configuration`): an `ApplicationConfigurationSnapshot`, the settings (`options`), whether the
  platform asks for less motion (`platform_reduced_motion`) and a revision counting their changes.

Commands:

- `replace_options(settings)` (TS `replaceOptions`): replaces the settings (not the startup-only name and
  description); whether they changed. Refused with `InvalidOptions` or `StartupOnlyOptionChanged`.

Rust helpers (on `dyn ApplicationConfigurationService`), synchronous functions of the snapshot whatever the
implementation:

- `snapshot()`, `options()`;
- `layout_direction()`;
- `effective_motion(local_request)`: an `EffectiveMotion` combining the platform's setting, the app's policy and a
  component's request (`is_reduced()`);
- `effective_programmatic_scroll_behavior(behavior, duration, motion)`: `Instant` when motion is reduced, else the
  caller's behaviour and duration over the app's.

## Use

```rust
let app_config = services.required_service::<dyn ApplicationConfigurationService>()?;
let direction = app_config.layout_direction();
let reduced = app_config.effective_motion(None).is_reduced();
let subscription = app_config.configuration().subscribe(|snapshot| relayout(snapshot));

let mut settings = app_config.options();
settings.layout.direction = LayoutDirection::Rtl;
app_config.replace_options(settings).await?;
```

Scrollbars and cursors are left to NSH and the platform.

## On NSH

The settings come from the app's bundled `appsettings.json` (`add_app_settings(package)`), read asynchronously, so the
app builds with `RayXAppBuilder::build_async`; starting it does that (Rust's `.start()` / `.start_async()`, TSX's
`await builder.start()`). The motion setting is the platform service's reduced-motion setting, followed as it
changes.

## TSX

`ApplicationConfigurationService`, resolved from the provider `start()` gives, wraps whichever
`dyn ApplicationConfigurationService` is registered:

- `current()`: the settings (their JSON, snake_case keys);
- `layoutDirection()` (`"ltr"` or `"rtl"`) and `reducedMotion()`, at once;
- `replaceOptions(settings)`: a promise of whether they changed; it takes all the settings, as `current()` gives
  them;
- `onChange(listener)`: hears the settings after every change; it returns a function that stops it.

```tsx
const settings = services.getRequiredService(ApplicationConfigurationService);
settings.layoutDirection(); // "ltr" or "rtl"
settings.reducedMotion();
const stop = settings.onChange((current) => apply(current));
await settings.replaceOptions({ ...settings.current(), layout: { direction: "rtl" } });
```

## Replace it

Register the app's own `dyn ApplicationConfigurationService` instead of calling `add_application_configuration` (it
then needs no platform service unless it uses one):

```rust
builder.add_singleton_as::<dyn ApplicationConfigurationService>(
    Arc::new(MyConfiguration::new()) as Arc<dyn ApplicationConfigurationService>,
)
```

In TS, a class implementing `ApplicationConfigurationServiceContract`, registered under the class
`ApplicationConfigurationService` ([contracts](../tsx.md#contracts)):

```tsx
class MyConfiguration {
  configuration = new ContractState({ options: { name: "TS App", layout: { direction: "rtl" } }, revision: 1 });
  async replaceOptions(options) {
    const now = this.configuration.value;
    this.configuration.publish({ ...now, options, revision: now.revision + 1 });
    return true;
  }
}

new RayXAppBuilder().addSingletonAs(ApplicationConfigurationService, () => new MyConfiguration());
```

Rust's helpers and the class's `layoutDirection()` and `reducedMotion()` then read its published state (a field left
out is its default).

`rayx_app::testing::FakeApplicationConfiguration` is a fake of the contract, publishing the snapshot a test gives it
and recording the settings `replace_options` was asked to take.
