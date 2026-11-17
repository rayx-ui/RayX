# Application configuration

The app's own settings: its name and description, motion, scrolling, tooltips, notifications and layout direction,
behind the `ApplicationConfigurationService` contract.

## Register

```rust
use rayx_services_application_configuration::ApplicationConfigurationAppBuilderExt;

builder
    .add_app_settings("my-app")
    .add_application_configuration(|options| options.notifications.auto_hide_ms = 0)
```

The contract is `rayx_app::services::application_configuration::ApplicationConfigurationService`; RayX's
implementation is `rayx_services_application_configuration`'s, registered as the app's
`dyn ApplicationConfigurationService`, replacing any other (`builder.addApplicationConfiguration(..)` in TS, once its
set is registered with `rayx_services_application_configuration::register(registry)`). An app may register its own, in
Rust or in TS (a class implementing `ApplicationConfigurationServiceContract`, registered under
`ApplicationConfigurationService`).

`add_application_configuration` requires the [platform service](platform.md) (`add_platform()` from
`rayx_services_platform`, or the app's own `dyn PlatformInfoService`), whose motion setting it follows, and registers
none; invalid settings fail startup.

## Configure

`ApplicationConfiguration` binds from the `RayX:Application` section, then the closure changes it:

```json
{
  "RayX": {
    "Application": {
      "name": "My App",
      "motion": { "policy": "System" },
      "tooltips": { "showDelayMs": 250 }
    }
  }
}
```

- The name and description apply at startup only.
- Unknown keys are ignored (as .NET does), so a shared `appsettings.json` may carry keys of other parts.
- On NSH the settings come from the app's bundled `appsettings.json` (`add_app_settings`), read asynchronously, so the
  app builds with `RayXAppBuilder::build_async`; starting it does that (Rust's `.start()` / `.start_async()`, TSX's
  `await builder.start()`).

## Use

```rust
let app_config = services.required_service::<dyn ApplicationConfigurationService>()?;
let direction = app_config.layout_direction();
let motion = app_config.effective_motion(None);
```

- The service publishes an `ApplicationConfigurationSnapshot` (`configuration()`, a state cell): the settings, whether
  the platform asks for less motion, and a revision counting their changes. It follows the configuration as it
  reloads (an options monitor) and the platform's motion setting; `configuration().subscribe(..)` hears each change.
- `layout_direction`, `effective_motion` and `effective_programmatic_scroll_behavior` are functions of the snapshot,
  synchronous whatever the implementation. The effective motion combines the system's setting, the app's policy and a
  component's request; the programmatic scroll behaviour follows it.
- `replace_options(settings).await` replaces the settings (not the name and description).
- Scrollbars and cursors are left to NSH and the platform.

## TSX

`ApplicationConfigurationService`, resolved from the provider `start()` gives:

```tsx
const settings = services.getRequiredService(ApplicationConfigurationService);
settings.layoutDirection(); // "ltr" or "rtl"
settings.reducedMotion();
settings.onChange((current) => apply(current));
await settings.replaceOptions({ ...settings.current(), layout: { direction: "rtl" } });
```
