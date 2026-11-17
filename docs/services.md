# App services

An app service is one piece of what an app needs (storage, navigation, translations, …) living in the app's
[dependency injection](dependency-injection.md) container. RayX's services are crates of their own (`rayx_services_<name>`) on
the core's contracts; an app's own services follow the same pattern.

## The services

| Service | What it does | Registered with | Options section |
| --- | --- | --- | --- |
| [I18n](services/i18n.md) | MessageFormat 2 translations; numbers, dates and plurals for the user's locale | `add_i18n(..)` | `RayX:Services:I18N` |
| [Operations](services/operations.md) | Typed domain operations applied to targets, with inverses | `add_operations()` (`rayx_services_operations`) | |
| [History](services/history.md) | Undo and redo per scope, in memory or persisted | `add_history()`, `add_persistent_history()` (`rayx_services_history`) | |
| [Storage](services/storage.md) | Values by bucket and key, persisted on NSH | `add_storage()`, `add_in_memory_storage()` | |
| [Navigation](services/navigation.md) | Routes, regions with journals, one location for the app | `add_navigation(..)` | `RayX:Services:Navigation` |
| [Selection](services/selection.md) | The active selection area and its status | `add_selection()` | |
| [Input map](services/input-map.md) | Key and pointer bindings per context, in three layers | `add_input_map(..)` | `RayX:Services:InputMap` |
| [Platform](services/platform.md) | The device and the user's settings | `add_platform()` | |
| [Suspension](services/suspension.md) | The app's lifecycle, focus and visibility, the system's wakes, quitting | `add_suspension()` | |
| [Application configuration](services/application-configuration.md) | The app's own settings: motion, scrolling, tooltips, layout direction | `add_application_configuration(..)` | `RayX:Application` |
| [Assets](services/assets.md) | The packages' files, from a manifest, as `asset:/` URLs | `add_assets(..)` | `RayX:Services:Assets` |
| [Fonts](services/fonts.md) | Fonts registered at run time, and aliases | `add_fonts(..)` | `RayX:Services:Fonts` |

Not available yet: **collaboration** (shared sessions over one storage, with other participants' operations applied
through the operation service). It will come with a standalone dXn collaboration engine.

## How a service works

**Registered with a builder extension.** Each service adds methods to the app builder through an `AppBuilder`
extension trait, so an app registers it in one call in its composition root, and TSX's `RayXAppBuilder` has the same
method in camelCase:

```rust
RayXAppBuilder::new()
    .add_app_settings("my-app")
    .add_storage()
    .add_operations()
    .add_persistent_history()
    .add_i18n(|options| {
        options.default_language("en");
    })
    .start();
```

```tsx
const services = await new RayXAppBuilder()
  .addAppSettings("my-app")
  .addStorage()
  .addOperations()
  .addPersistentHistory()
  .addI18n((options) => options.defaultLanguage("en"))
  .start();
```

`add_<service>(..)` keeps a service registered earlier; `with_<service>_service(..)` replaces it, for tests or a
custom implementation. A service registers no other: it requires what it depends on (`add_i18n` requires the asset
and platform services, `add_history` an operation service, `add_operations()`, and `add_input_map` and
`add_persistent_history` a key-value store, `add_storage()`).

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

**Resolved by type.** Rust code resolves a service from the provider: `services.required_service::<dyn I18nService>()`,
or `rayx_app_nsh::services()` in a component. TSX resolves them by their classes, named after the Rust services, from
the provider `start()` gives: `services.getRequiredService(I18nService)` ([TSX](tsx.md#rayxs-services)).

**Written in TS too.** An app's own services may be TS classes in the same container, and a Rust trait marked
`#[rayx::contract]` lets Rust call a TS implementation ([TSX](tsx.md#services-written-in-ts)).

**Contracts in the core, implementations in crates.** A service's trait and data types live in `rayx_app::services`
(NSH-free, building for `wasm32-unknown-unknown`), and RayX's implementation in a crate of its own: the platform,
suspension, asset, font, storage, navigation and i18n services are `rayx_services_platform`,
`rayx_services_suspension`, `rayx_services_assets`, `rayx_services_fonts`, `rayx_services_storage`,
`rayx_services_navigation` and `rayx_services_i18n`, each registering itself as `dyn Trait` (`add_platform()`,
`add_suspension()`, `add_assets(..)`, `add_fonts(..)`, `add_storage()`, `add_navigation(..)`, `add_i18n(..)`, and
`builder.addPlatform()` and the others in TS, once the crate's set is registered). I18n is two traits: the
`I18nService` contract and the Rust-only `MessageFormatter`, which `add_i18n` registers too
([I18n](services/i18n.md)). A service that
needs another resolves it by trait, so an app's own implementation, in Rust or TS, replaces RayX's anywhere;
`rayx_app::testing` has a fake of each trait.

## On NSH

`rayx_app_nsh` gives the services what they need from NSH:

- The app's start requires the platform, suspension, asset and font services: a missing one fails the start before
  anything is made, naming each one missing and how to add it. Nothing is registered for the app: `RayXAppBuilder::new()`
  and `new RayXAppBuilder()` are empty.
- The app's start registers NSH's location where the app registered none. The key-value store on NSH's storage is
  `rayx_services_storage`'s (`add_storage()`).
- `add_app_settings(package)` adds the package's bundled `appsettings.json` as a configuration source, loaded
  asynchronously when the app starts.
- The [platform](services/platform.md) and [suspension](services/suspension.md) services keep themselves current from
  NSH from their startup on, in Rust and TSX apps alike; the [asset](services/assets.md) and [font](services/fonts.md)
  services read and register through NSH from any thread, and so does the [key-value store](services/storage.md),
  which writes what waits as the app quits; the [i18n](services/i18n.md) formatter keeps `tr!` and NSH's locale on
  the i18n service from its startup on.

## Writing a service

A service crate depends on `rayx_app` alone and follows the same pattern: options bound from a section, a service
made from them, and a builder extension that registers both.

```rust
use rayx_app::prelude::*;
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

An app then calls `builder.add_weather(|options| options.refresh_minutes = 15)`. To reach TS, an app exports such a
function as an [app extension](tsx.md#app-extensions) (`#[rayx::app_extension]`, a function of the builder alone),
which TS calls as a builder method, `builder.addWeather()`.
