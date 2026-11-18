# App services

An app service is one piece of what an app needs (storage, navigation, translations, …) living in the app's
[dependency injection](dependency-injection.md) container. RayX's services are crates of their own (`rayx_services_<name>`) on
the core's contracts; an app's own services follow the same pattern.

## The services

Each service is a contract in the core (`rayx_app::services::<name>`, also `rayx::app::services::<name>`) and RayX's
implementation in the crate `rayx_services_<name>` (`rayx::services::<name>`), which the umbrella `rayx` builds with
the feature `<name>`. The features marked default are the four services the NSH host requires at the start.

| Service | What it does | Feature | Registered with (Rust; TS) | Requires | Options section |
| --- | --- | --- | --- | --- | --- |
| [Platform](services/platform.md) | The device and the user's settings | `platform` (default) | `add_platform()`; `addPlatform()` | | |
| [Suspension](services/suspension.md) | The app's lifecycle, focus and visibility, the system's wakes, quitting | `suspension` (default) | `add_suspension()`; `addSuspension()` | | |
| [Assets](services/assets.md) | The packages' files, from a manifest, as `asset:/` URLs | `assets` (default) | `add_assets(..)`; `addAssets(..)` | | `RayX:Services:Assets` |
| [Fonts](services/fonts.md) | Fonts registered at run time, and aliases | `fonts` (default) | `add_fonts(..)`; `addFonts(..)` | assets | `RayX:Services:Fonts` |
| [Storage](services/storage.md) | Values by bucket and key, persisted on NSH | `storage` | `add_storage()`, `add_in_memory_storage()`; `addStorage()`, `addInMemoryStorage()` | | |
| [Navigation](services/navigation.md) | Routes, regions with journals, one location for the app | `navigation` | `add_navigation(..)`; `addNavigation(..)` | | `RayX:Services:Navigation` |
| [I18n](services/i18n.md) | MessageFormat 2 translations; numbers, dates and plurals for the user's locale | `i18n` | `add_i18n(..)`, `add_message_formatter()`; `addI18n(..)`, `addMessageFormatter()` | assets, platform | `RayX:Services:I18N` |
| [Operations](services/operations.md) | Typed domain operations applied to targets, with inverses | `operations` | `add_operations()`; `addOperations()` | | |
| [History](services/history.md) | Undo and redo per scope, in memory or persisted | `history` | `add_history()`, `add_persistent_history()`, `add_history_with_store(..)`; `addHistory(..)`, `addPersistentHistory(..)`, `addHistoryWithStore(..)` | operations (persisted: storage) | `RayX:Services:History` |
| [Selection](services/selection.md) | The active selection area and its status | `selection` | `add_selection()`; `addSelection()` | navigation | |
| [Input map](services/input-map.md) | Key and pointer bindings per context, in three layers | `input_map` | `add_input_map(..)`; `addInputMap(..)` | storage | `RayX:Services:InputMap` |
| [Application configuration](services/application-configuration.md) | The app's own settings: motion, scrolling, tooltips, layout direction | `application_configuration` | `add_application_configuration(..)`; `addApplicationConfiguration(..)` | platform | `RayX:Application` |

"Requires" names the services RayX's implementation resolves: the app registers each, RayX's or its own, or the build
fails naming it ([Startup](startup.md#how-the-app-starts)). No feature implies another.

Not available yet: **collaboration** (shared sessions over one storage, with other participants' operations applied
through the operation service). It will come with a standalone dXn collaboration engine.

## How a service works

**Registered with a builder extension.** Each service adds methods to the app builder through an `AppBuilder`
extension trait, so an app registers it in one call in its composition root, and TSX's `RayXAppBuilder` has the same
method in camelCase:

```toml
[dependencies]
rayx = { version = "…", features = ["storage", "operations", "history", "i18n"] }
```

```rust
RayXAppBuilder::new()
    .add_app_settings("my-app")
    .add_platform()
    .add_suspension()
    .add_assets(|options| {
        options.package("my-app");
    })
    .add_fonts(|_| {})
    .add_storage()
    .add_operations()
    .add_persistent_history()
    .add_i18n(|options| {
        options.default_language("en");
    })
    .start();
```

```tsx
new RayXAppBuilder()
  .addAppSettings("my-app")
  .addPlatform()
  .addSuspension()
  .addAssets((options) => options.package("my-app"))
  .addFonts()
  .addStorage()
  .addOperations()
  .addPersistentHistory()
  .addI18n((options) => options.defaultLanguage("en"))
  .start()
  .then((services) => { /* resolve the services here */ });
```

`add_<service>(..)` registers RayX's implementation as the app's `dyn Trait`, replacing any registered before. A
service registers no other: it declares what it depends on (`add_i18n` the asset and platform services, `add_fonts`
the asset service, `add_history` an operation service, `add_persistent_history` and `add_input_map` a key-value store,
`add_selection` a navigation service, `add_application_configuration` the platform service), and the build fails
naming each one missing with how to add it.

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

**Contracts in the core, implementations in crates.** A service's trait and data types live in
`rayx_app::services::<name>` (NSH-free, building for `wasm32-unknown-unknown`), each a `#[contract]` trait whose
members are published state, published events, constants and commands ([TSX](tsx.md#contracts)); RayX's
implementation is its crate `rayx_services_<name>`, which registers itself as the app's `dyn Trait` and adds the TS
side to the app's set: the service's class (its token, named after the trait), the contract's interface
(`<Name>Contract`) and its builder method. I18n is two traits: the `I18nService` contract and the Rust-only
`MessageFormatter`, which `add_i18n` registers too ([I18n](services/i18n.md)). A service that needs another resolves
it by trait, so an app's own implementation, in Rust or TS, replaces RayX's for every consumer.

## Replacing a service

An app replaces RayX's implementation of a service by registering its own as the trait instead of calling the
service's builder method. RayX's other services, the host's requirement and the service's TS class then use it.

```rust
builder.add_singleton_as::<dyn KeyValueStore>(Arc::new(KeychainStore::new()) as Arc<dyn KeyValueStore>)
```

One that must initialize at the start registers with `add_startup_singleton_for::<dyn Trait, Impl>(Arc<Impl>)`
instead, as RayX's do: the trait and a startup service registered as it, so the services that wait for the trait wait
for it ([Startup](startup.md#implementing-a-trait)):

```rust
builder.add_startup_singleton_for::<dyn KeyValueStore, _>(Arc::new(KeychainStore::new()))
```

Registered before or after the service's builder method, the last registration wins completely: registering the trait
removes the startup services registered as it before, so RayX's never starts once the app's own replaced it.

In TS, a class implementing the contract's interface, registered under the service's class: published state as a
`ContractState`, events as `ContractEvents`, constants as fields, commands as `async` methods
([TSX](tsx.md#implementing-a-service-in-ts)):

```tsx
class KeychainStore implements KeyValueStoreContract {
  capabilities = { persistent: true, secure: true };
  async get(bucket: string, key: string) { /* … */ return null; }
  async set(bucket: string, key: string, value: number[]) { /* … */ }
  async remove(bucket: string, key: string) { return false; }
  async keys(bucket: string) { return []; }
  async clear(bucket: string) {}
}

new RayXAppBuilder().addSingletonAs(KeyValueStore, () => new KeychainStore());
```

A TS implementation's values cross as JSON, bytes as they are: where a contract answers bytes
(`number[] | Uint8Array`, as `loadBytes` and a store's `get`), TS may return a `Uint8Array` (any typed array, an
`ArrayBuffer`) or a `number[]`. `rayx_app::testing` has a fake of each trait (`FakePlatform`, `FakeKeyValueStore`, …)
for tests. Each service's page has its own example under "Replace it".

## On NSH

The NSH host, `rayx_app_nsh`, starts the app; the service crates do the rest on NSH:

- The app's start requires the platform, suspension, asset and font services: a missing one fails the start before
  anything is made, naming each one missing and how to add it. Nothing is registered for the app:
  `RayXAppBuilder::new()` and `new RayXAppBuilder()` are empty.
- `add_app_settings(package)` (the host's) adds the package's bundled `appsettings.json` as a configuration source,
  loaded asynchronously when the app starts.
- The [platform](services/platform.md) and [suspension](services/suspension.md) services keep themselves current from
  NSH from their startup on, in Rust and TSX apps alike; the [asset](services/assets.md) and [font](services/fonts.md)
  services read and register through NSH from any thread, and so does the [key-value store](services/storage.md),
  which writes what waits as the app quits; the [navigation](services/navigation.md) service shows its location as
  NSH's app address; the [i18n](services/i18n.md) formatter keeps `tr!` and NSH's locale on the i18n service from its
  startup on.

## Writing a service

An app's own service follows the same pattern: options bound from a section, a service made from them, and a builder
extension that registers both. A crate of services that need no NSH depends on `rayx_app` alone (and builds for
`wasm32-unknown-unknown`); one with an NSH side depends on `rayx_app_nsh` too, as RayX's service crates do.

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
        builder.services_mut().replace_service_factory::<WeatherService>(|services| {
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
