# TSX

A TSX app is written the way a Rust app is: it builds its services with `RayXAppBuilder`, starts the app, and resolves
services from the `ServiceProvider` the start gives it, with the same lifetimes, scopes and startup. Every TS member is
the camelCase twin of the Rust one (`add_i18n` is `addI18n`, `required_service` is `getRequiredService`), and each RayX
service's TS class has the Rust service's name.

## The app's crate

A TSX app keeps a Rust crate for its UI set (`nsh.json` `"rust"`). Its root is RayX's attribute on one function:

```rust
use rayx::prelude::*;

#[rayx::native_module("my-app")]
fn native(registry: &mut Registry) {}
```

It is NSH's `#[native_module("my-app")]` on the function, whose body first registers every RayX TS class
(`rayx::app_nsh::register`: the builder, the provider and its tokens, the startup classes and interfaces, the options
classes and the services' classes) and RayX's components with their theme document (`rayx::components::register`),
and nothing else. The function's own body adds the app's exports:

```rust
#[rayx::native_module("my-app")]
fn native(registry: &mut Registry) {
    registry
        .register::<Chart>()                     // a component
        .class_extension::<add_weather>()        // an app extension
        .contract::<dyn WeatherSource>()         // a contract
        .class::<DevTools>()
        .register::<DevLayer>();                 // RayX's dev tools
}
```

TSX imports the set by its specifier (`import { RayXAppBuilder } from "my-app"`), and the crate gains `pub fn
registry()`, as with NSH's attribute. Keep the attribute's line in the crate's source: NSH's CLI reads the set's
specifier from it, and the web runtime keeps the set by that name. A hand-written `#[native_module]` registers the same
with `rayx::app_nsh::register(registry)` and `rayx::components::register(registry)`. If the crate depends on `nsh-core`,
depend on it in `[dependencies]` (or not at all): the attribute takes NSH's core from the crate's own dependency when it
names one, so `nsh-core` in `[dev-dependencies]` alone breaks the crate's normal build.

## Starting the app

Rust, in the root component:

```rust
#[component]
pub fn App() -> impl IntoElement {
    RayXAppBuilder::new()
        .add_app_settings("my-app")
        .add_storage()
        .add_i18n(|options| {
            options.default_language("en");
        })
        .start();
    // ... `app_state()` says when it is ready
}
```

TSX, before the app uses a service:

```tsx
import { I18nService, RayXAppBuilder } from "my-app";

const services = await new RayXAppBuilder()
  .addAppSettings("my-app")
  .addStorage()
  .addI18n((options) => options.defaultLanguage("en"))
  .start();
const i18n = services.getRequiredService(I18nService);
```

- `start()` resolves with the surface's root `ServiceProvider` once every startup service, Rust's and TS's, initialized
  and every ready hook ran. It rejects with an `Error` whose message is the startup error's: a failing service, a
  timeout, an option validated on start, a ready hook.
- A second `start()` on the same surface, of the same builder or another, resolves with the same root. After
  `start()`, the builder's other methods throw.
- Options callbacks (`addI18n((options) => …)`) take the Rust options' methods in camelCase, and apply after the
  configuration binds, as in Rust.
- `defaultInitializationTimeout(ms)` and every TS timeout are in milliseconds.
- `start()` requires the platform, suspension, asset and font services (`addPlatform()`, `addSuspension()`,
  `addAssets()`, `addFonts()` from their crates' sets, or the app's own) and rejects naming each one missing.

See [Startup](startup.md).

## Resolving services

| Rust (`rayx_app::ServiceProvider`) | TS (`ServiceProvider`) |
| --- | --- |
| `services.service::<I18nService>()` | `services.getService(I18nService)`: the service, or `null` |
| `services.required_service::<I18nService>()` | `services.getRequiredService(I18nService)`: throws an `Error` naming the token when none is registered |
| `services.options::<FeedOptions>()` | `services.getOptions(Feed).value` |
| `services.create_scope()` | `services.createScope()` |
| dropping the scope | `scope.dispose()` |

A token is a `ServiceKey<T>`: a class (`I18nService`, or a TS class) or a `ServiceToken<T>`. Results are typed by it,
so no call site casts. A screen makes its scope when it mounts and disposes it in its cleanup; disposing drops the
scoped services it resolved and deactivates the view models activated through it, and using it afterwards throws.

**Identity.** A TS singleton is one JS object per root, so `===` holds across every scope. A RayX service's class is a
wrapper each provider makes around the Rust service: `===` holds for a singleton resolved twice through the same
provider, but the root and a scope give two wrappers of the one Rust singleton. Compare Rust services by identity only
within one provider; the wrappers behave the same, since they share the Rust service.

```tsx
const scope = services.createScope(); // on a screen's mount
const editor = scope.getRequiredService(EditorState);
scope.dispose();                      // in its cleanup
```

A Rust bridge takes the provider as `Instance<ServiceProvider>` and resolves in it:
`services.with(|services| services.services().required_service::<MyService>())`.

## RayX's services

Each is resolved by its class, and its `onChange` (`onLifecycle`, `askBeforeQuit`) returns a function that stops the
listener.

| Class | Rust service | What it does |
| --- | --- | --- |
| `I18nService` | `I18nService` | `t(id, args)`, `has(id)`, `formatDate(instant, options)` in the user's time zone, `formatNumber(value, options)`, `timeZone()`, `locale()`, `languages()`, `direction()`, `setLanguage(language)` ([I18n](services/i18n.md#tsx)) |
| `NavigationService` | `NavigationService` | The URL region: `navigate(uri)`, `back()`, `forward()`, `current()`; `region(name)` gives a `NavigationRegion` ([Navigation](navigation.md#from-tsx)) |
| `KeyValueStore` | `dyn KeyValueStore` | Text and JSON values by bucket and key, in NSH's storage: `get`, `set`, `getJson`, `setJson`, `remove`, `keys`, `clear` ([Storage](services/storage.md#tsx)) |
| `ApplicationConfigurationService` | `ApplicationConfigurationService` | The app's settings: `current()`, `layoutDirection()`, `reducedMotion()` |
| `AssetService` | `AssetService` | Asset URLs: `url(path)`, `urlForScale(path, scale)` |
| `FontService` | `FontService` | Font aliases: `family(alias)`, `aliases()` |
| `PlatformInfoService` | `PlatformInfoService` | The device: `current()`, `onChange` ([Platform](services/platform.md#tsx)) |
| `SuspensionService` | `SuspensionService` | The app's lifecycle: `lifecycle()`, `onLifecycle`, `askBeforeQuit(listener)`, `quit()` ([Suspension](services/suspension.md#tsx)) |

```tsx
const i18n = services.getRequiredService(I18nService);
title.set(i18n.t("welcome", { name: "Ada" }));
const stop = i18n.onChange(() => title.set(i18n.t("welcome", { name: "Ada" })));

services.getRequiredService(KeyValueStore).setJson("settings", "theme", { mode: "dark" });
```

## Services written in TS

TS services live in RayX's one container: the same lifetimes and scopes as Rust's, the same startup graph, timeout
and failure report. A TS class is its own token; a `ServiceToken` names a service by something else (an interface).

Rust:

```rust
builder
    .add_singleton(Clock::default())
    .add_scoped(|services| Page::new(services.required_service::<Clock>().unwrap()))
    .add_singleton_as::<dyn Greeter>(Arc::new(EnglishGreeter) as Arc<dyn Greeter>)
```

TSX:

```tsx
const Greeter = new ServiceToken<Greeter>("Greeter");

new RayXAppBuilder()
  .addSingleton(Clock, () => new Clock())
  .addScoped(Page, (services) => new Page(services.getRequiredService(Clock)))
  .addSingletonAs(Greeter, () => new EnglishGreeter());
```

- Every Rust registration has its twin: `addSingleton`, `addScoped`, `addTransient`, their `As` forms, and the
  `addStartup…` forms ([Startup](startup.md#startup-services-in-ts)). A factory gets the resolving provider and may
  resolve TS and Rust services; one that throws fails the resolve with its message.
- A RayX service's class is not a token for a TS service: register under a TS class or a `ServiceToken`.
- **The engine thread.** TS services are made and resolved on the engine thread only. A TS service is made in a call
  from JS (a TS `getService`); startup and contract services are made by `start()`. Rust resolving a TS service off
  the engine thread fails (`ServiceResolutionError::WrongThread`); resolving one that is not made yet, outside a call
  from JS, fails too. To Rust, a TS service is an opaque entry; Rust calls it only through a contract.

## Contracts

A contract is a Rust trait TS implements, which Rust resolves as `Arc<dyn Trait>`:

```rust
#[rayx::contract]
pub trait WeatherSource {
    /// The forecast for `city`.
    async fn forecast(&self, city: String) -> Result<String, WeatherError>;
    /// Notes `line`.
    fn note(&self, line: String);
}

// In the app's set: `registry.contract::<dyn WeatherSource>()`.
let source = services.required_service::<dyn WeatherSource>()?;
let forecast = source.forecast("Rijeka".into()).await?;
```

```tsx
import { RayXAppBuilder, weatherSourceToken, type WeatherSource } from "my-app";

class OpenMeteo implements WeatherSource {
  async forecast(city: string) { return `sunny in ${city}`; }
  note(line: string) { console.log(line); }
}

new RayXAppBuilder().addSingletonAs(weatherSourceToken(), () => new OpenMeteo());
```

- TS gets the interface `WeatherSource` and the token function `weatherSourceToken()`; every registration form takes
  the token, and Rust resolves the implementation with the same lifetime. Rust startup services that depend on
  `dyn WeatherSource` initialize after it.
- An `async fn` returns `Result<T, E>`, where `E: From<ContractError> + From<Exception>` (`String` is one): what TS
  throws arrives as the `Exception`. A method returning nothing runs on the spot inside a call from JS and is posted, in
  order, otherwise.
- Calls reach TS from any thread: off the engine thread they go through the surface's queue (`EngineRunner`), and
  once the engine stopped they fail with `ContractError::EngineGone`.
- A contract has no generics, associated items, supertraits or default bodies; its values cross to JS (`String`,
  numbers, `#[derive(JsObject)]` structs).

## Options

TS options are a token with their defaults, configured as Rust's are (see
[Configuration and options](configuration.md#options-in-ts)):

```tsx
const Feed = new OptionsToken("Feed", { title: "News", pageSize: 10 });

const services = await new RayXAppBuilder()
  .addOptions(Feed, (options) => options
    .bind("Feed")
    .validate((feed) => feed.pageSize > 0, "pageSize must be positive")
    .validateOnStart())
  .configureOptions(Feed, (feed) => ({ ...feed, title: feed.title.trim() }))
  .start();
const feed = services.getOptions(Feed).value;
```

## App extensions

An app's own Rust services reach TS through an app extension: a plain Rust function of the builder, which a Rust root
calls (or passes to `builder.apply(..)`) and TS applies by its TS name.

```rust
/// Registers the weather service.
#[rayx::app_extension]
pub fn add_weather(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Weather::default())
}

#[rayx::native_module("my-app")]
fn native(registry: &mut Registry) {
    registry.class_extension::<add_weather>();
}
```

```tsx
import { RayXAppBuilder } from "my-app";

const services = await new RayXAppBuilder().addStorage().addWeather().start();
```

The method runs the Rust function on the builder and returns the builder, and throws once the builder started.
`apply` takes a TS function of the builder, and runs it in chain order.

## View models

A view-model class exposes `Observable`s and `ReactiveCommand`s; see [View models](view-models.md#binding-in-tsx).

## Components

RayX's components (`Button`) and its theme document are in the set `#[rayx::native_module]` makes; import them by its
specifier. See [Components and theme](components.md).

```tsx
import { Button } from "my-app";

<Button text="Save changes" variant="primary" onPress={save} />
```
