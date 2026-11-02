# Application configuration

The app's own settings: its name and description, motion, scrolling, tooltips, notifications and layout direction.

## Register

```rust
builder
    .add_app_settings("my-app")
    .add_application_configuration(|options| options.notifications.auto_hide_ms = 0)
```

`add_application_configuration` registers the [platform service](platform.md) too; invalid settings fail startup.

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
  app builds with `RayXAppBuilder::build_async` (`rayx_app::start_app` does).

## Use

```rust
let app_config = services.required_service::<ApplicationConfigurationService>()?;
let direction = app_config.layout_direction();
let motion = app_config.effective_motion(None);
```

- The service follows the configuration as it reloads (an options monitor) and notifies its listeners.
- The effective motion combines the system's setting (from the platform service), the app's policy and a component's
  request; the programmatic scroll behaviour follows it.
- Scrollbars and cursors are left to NSH and the platform.

## TSX

```tsx
const settings = AppConfiguration.of(app);
settings.layoutDirection(); // "ltr" or "rtl"
settings.reducedMotion();
settings.onChange((current) => apply(current));
```
