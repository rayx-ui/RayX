# App services

An app service is one piece of what an app needs (storage, navigation, translations, …) living in the app's
[dependency injection](dependency-injection.md) container. RayX's services are in `rayx_runtime`; an app's own services
follow the same pattern.

## The services

| Service | What it does | Registered with | Options section |
| --- | --- | --- | --- |
| [I18n](services/i18n.md) | MessageFormat 2 translations; numbers, dates and plurals for the user's locale | `add_i18n(..)` | `RayX:Services:I18N` |
| [Operations](services/operations.md) | Typed domain operations applied to targets, with inverses | `add_operations()` | |
| [History](services/history.md) | Undo and redo per scope, in memory or persisted | `add_history()`, `add_persistent_history()` | |
| [Storage](services/storage.md) | Values by bucket and key, persisted on NSH | `add_storage()` | |
| [Navigation](services/navigation.md) | Routes, regions with journals, one location for the app | `add_navigation(..)` | `RayX:Services:Navigation` |
| [Selection](services/selection.md) | The active selection area and its status | `add_selection()` | |
| [Input map](services/input-map.md) | Key and pointer bindings per context, in three layers | `add_input_map(..)` | `RayX:Services:InputMap` |
| [Platform](services/platform.md) | The device and the user's settings | `add_platform()` | |
| [Suspension](services/suspension.md) | The app's lifecycle, focus and visibility | `add_suspension()` | |
| [Application configuration](services/application-configuration.md) | The app's own settings: motion, scrolling, tooltips, layout direction | `add_application_configuration(..)` | `RayX:Application` |
| [Assets](services/assets.md) | The packages' files, from a manifest, as `asset:/` URLs | `add_assets(..)` | `RayX:Services:Assets` |
| [Fonts](services/fonts.md) | Fonts registered at run time, and aliases | `add_fonts(..)` | `RayX:Services:Fonts` |

Not available yet: **collaboration** (shared sessions over one storage, with other participants' operations applied
through the operation service). It will come with a standalone dXn collaboration engine.

## How a service works

**Registered with a builder extension.** Each service adds methods to the app builder through an `AppBuilder`
extension trait, so an app registers it in one call in its composition root:

```rust
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .add_app_settings("my-app")
        .add_storage()
        .add_persistent_history()
        .add_i18n(|options| {
            options.default_language("en");
        })
}
```

`add_<service>(..)` keeps a service registered earlier; `with_<service>_service(..)` replaces it, for tests or a
custom implementation. A service registers what it depends on: `add_i18n` adds the asset
service, `add_history` the operation service.

**Configured through options.** A service reads its options with RayX's [options model](configuration.md) from the
section `RayX:Services:<Name>`. The configuration binds first, then the closure passed to `add_<service>(..)` changes
the result, so `appsettings.json` sets the defaults and code has the last word. Keys match ignoring case, so
`appsettings.json` may spell them in camelCase:

```json
{ "RayX": { "Services": { "I18N": { "defaultLanguage": "en" } } } }
```

**Ready at startup.** Services that need to load something (assets, translations, fonts) are
[startup services](startup.md): they initialize in dependency order before the app is ready, and invalid options fail
startup instead of the first use.

**Resolved by type.** Rust code resolves a service from the provider: `services.required_service::<I18nService>()`, or
`rayx_app::services()` in a component. TSX reaches the services through classes made with `X.of(app)`
([TSX](tsx.md)).

**Platform-independent.** `rayx_runtime` has no NSH dependency and builds for `wasm32-unknown-unknown`. What a service
needs from the platform is a trait (`AssetReader`, `FontRegistrar`, `KeyValueStore`,
`NavigationLocationSynchronizer`) that `rayx_app` implements on NSH, so the services stay testable with `cargo test`.

## On NSH

`rayx_app` gives the services what they need from NSH:

- Every surface's builder starts with `NshAssetReader` (reads `asset:/` URLs with `fetch`), `NshFontRegistrar`
  (registers fonts with `register_font`) and `NshKeyValueStore` (persists values in NSH's storage).
- `add_app_settings(package)` adds the package's bundled `appsettings.json` as a configuration source, loaded
  asynchronously when the app starts.
- `use_platform_sync()`, in a Rust root component, keeps the [platform](services/platform.md) and
  [suspension](services/suspension.md) services current from NSH once the app is ready. A TSX app has no root
  component, so `AppServices.load()` does it.

## Writing a service

A service crate depends on `rayx_core` alone and follows the same pattern: options bound from a section, a service
made from them, and a builder extension that registers both.

```rust
use rayx_core::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct WeatherOptions {
    pub endpoint: String,
    pub refresh_minutes: u32,
}

#[derive(AppService)]
pub struct WeatherService {
    options: WeatherOptions,
}

pub trait WeatherAppBuilderExt: AppBuilder {
    fn add_weather(self, configure: impl Fn(&mut WeatherOptions) + Send + Sync + 'static) -> Self {
        let mut builder = self.add_options::<WeatherOptions>(|options| {
            options
                .bind("Weather")
                .configure(configure)
                .validate(|options| !options.endpoint.is_empty(), "the weather service needs an endpoint")
                .validate_on_start();
        });
        builder.services_mut().try_add_service_factory::<WeatherService>(|services| {
            let options = services.options::<WeatherOptions>().expect("validated on start");
            WeatherService { options: options.cloned() }
        });
        builder
    }
}

impl<T> WeatherAppBuilderExt for T where T: AppBuilder {}
```

An app then calls `builder.add_weather(|options| options.refresh_minutes = 15)`.
