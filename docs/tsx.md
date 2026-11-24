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

It is NSH's `#[native_module("my-app")]` on the function, whose body first registers RayX's TS classes
(`rayx::app_nsh::register`: the builder, the provider and its tokens, the startup classes and interfaces, the
configuration and options classes, `ContractState` and `ContractEvents`), the set of each service crate the umbrella's
features enable (its class, its contract's interface, its options classes and its builder method:
`rayx_services_i18n::register` gives `addI18n(..)`, `addMessageFormatter()`, `I18nService`, `I18nServiceContract` and
`I18nOptions`, and `rayx_services_theme::register` gives `addTheme(..)`, `ThemeService`, `ThemeServiceContract` and
`ThemeOptions`), and nothing else, and serves RayX's component library, the set `rayx/components`
(`registry.serve(rayx::components::set())`: the components, `ComponentMeta<C>` and RayX's theme document). With
the default features the builder has `addPlatform()`, `addSuspension()`, `addAssets(..)` and `addFonts(..)`; a feature
off leaves its method and class out of the set and its declarations. The function's own body adds the app's exports:

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

TSX imports the set by its specifier (`import { RayXAppBuilder } from "my-app"`), and RayX's components by the
library's (`import { Button } from "rayx/components"`); the crate gains `pub fn registry()`, as with NSH's attribute.
Keep the attribute's line in the crate's source: NSH's CLI reads the set's specifier from it, and the web runtime keeps
the set by that name. A hand-written `#[native_module]` registers the same with `rayx::app_nsh::register(registry)`,
each service crate's `register(registry)` (`rayx::services::storage::register(registry)`, …) and serves the library
with `registry.serve(rayx::components::set())`. If the crate depends on `nsh-core`, depend on it in `[dependencies]` (or
not at all): the attribute takes NSH's core from the crate's own dependency when it names one, so `nsh-core` in
`[dev-dependencies]` alone breaks the crate's normal build.

## Starting the app

Rust, in the root component:

```rust
#[component]
pub fn App() -> impl IntoElement {
    RayXAppBuilder::new()
        .add_app_settings("my-app")
        .add_platform()
        .add_suspension()
        .add_assets(|_| {})
        .add_fonts(|_| {})
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

new RayXAppBuilder()
  .addAppSettings("my-app")
  .addPlatform()
  .addSuspension()
  .addAssets()
  .addFonts()
  .addStorage()
  .addI18n((options) => options.defaultLanguage("en"))
  .start()
  .then((services) => {
    const i18n = services.getRequiredService(I18nService);
  });
```

- Chain `.then` on `start()` (as `rayx new`'s `App.tsx` does), or await it inside an `async` function: never at a
  module's top level, which `nsh run`'s development bundle (CommonJS) cannot run.
- `start()` resolves with the surface's root `ServiceProvider` once every startup service, Rust's and TS's, initialized
  and every ready hook ran. It rejects with an `Error` whose message is the startup error's: a failing service, a
  timeout, an option validated on start, a ready hook.
- A second `start()` on the same surface, of the same builder or another, resolves with the same root. After
  `start()`, the builder's other methods throw.
- `restart()` starts the app on the surface afresh with another builder, in place of the one running there: what a
  design tool does when the app's composition (its `configure`) changed while it runs. A startup under way ends
  first; then the old root and every scope made from it with `createScope` are disposed (their view models
  deactivated, using them throws), `appState()` goes to `"starting"` and then `"ready"` or `"failed"`, and it resolves
  with the new root (or rejects) as `start()` does. A later `start()` gives the new root. Rust's twin is
  `builder.restart_async()` (`StartAppBuilderExt`).

  <!-- tsx: fragment -->
  ```tsx
  // The composition was reloaded: drop what was drawn with the old root, then start the new one.
  const root = await configure(new RayXAppBuilder()).restart();
  ```
- Options callbacks (`addI18n((options) => …)`) take the Rust options' methods in camelCase, and apply after the
  configuration binds, as in Rust.
- A service crate's builder method (`addNavigation(..)`, `addFonts(..)`, …) and class exist, in JS and in the
  declarations, once the crate's set is registered: the umbrella's feature of its name on (`features = ["navigation"]`),
  or `rayx_services_navigation::register(registry)` in a hand-written set. Each registers RayX's implementation,
  replacing any registered before.
- `defaultInitializationTimeout(ms)` and every TS timeout are in milliseconds.
- `start()` requires the platform, suspension, asset and font services (`addPlatform()`, `addSuspension()`,
  `addAssets()`, `addFonts()`, or the app's own implementations of their contracts) and rejects naming each one
  missing, with the TS method or contract that adds it; so does a service whose dependency is missing (`addFonts()`
  without an asset service).

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
within one provider; the wrappers behave the same, since they share the Rust service. A screen's scope (a fragment:
`services` is the root, `EditorState` one of the app's scoped services):

<!-- tsx: fragment -->
```tsx
const scope = services.createScope(); // on a screen's mount
const editor = scope.getRequiredService(EditorState);
scope.dispose();                      // in its cleanup
```

A Rust bridge takes the provider as `Instance<ServiceProvider>` and resolves in it:
`services.with(|services| services.services().required_service::<MyService>())`.

## RayX's services

Each is resolved by its class, which is the service's token and has the Rust trait's name; it wraps whichever
implementation is registered, RayX's or the app's own, Rust or TS. Queries answer at once; commands that wait on the
service return promises. Every `onChange` (`onLifecycle`, `onEvent`, `askBeforeQuit`) returns a function that stops the
listener (it also stops when the reactive owner of the call goes, or the object is disposed).

| Class | Rust trait | Members | Set (feature) |
| --- | --- | --- | --- |
| `PlatformInfoService` | `dyn PlatformInfoService` | `current()`, `onChange` ([Platform](services/platform.md#tsx)) | `platform` |
| `SuspensionService` | `dyn SuspensionService` | `lifecycle()`, `onLifecycle`, `askBeforeQuit(listener)`, `quit()` ([Suspension](services/suspension.md#tsx)) | `suspension` |
| `AssetService` | `dyn AssetService` | `url(path)`, `urlForScale(path, scale)` ([Assets](services/assets.md#tsx)) | `assets` |
| `FontService` | `dyn FontService` | `family(alias)`, `aliases()` ([Fonts](services/fonts.md#tsx)) | `fonts` |
| `KeyValueStore` | `dyn KeyValueStore` | `capabilities`; as promises: `get`, `set`, `getJson`, `setJson`, `remove`, `keys`, `clear` ([Storage](services/storage.md#tsx)) | `storage` |
| `NavigationService` | `dyn NavigationService` | The URL region: `current()`, `onChange`; as promises: `navigate(uri)`, `back()`, `forward()`, `refresh()`; `region(name)` gives a `NavigationRegion` (`pages`, `onChange`; as promises: `navigate`, `back`, `forward`, `refresh`, `goTo`, `close`) ([Navigation](navigation.md#from-tsx)) | `navigation` |
| `I18nService` | `dyn I18nService`, formatted by `dyn MessageFormatter` | `t(id, args)`, `has(id)`, `formatDate(instant, options)`, `formatNumber(value, options)`, `timeZone()`, `locale()`, `languages()`, `direction()`, `onChange`; as promises: `setLanguage(language)`, `followPlatform()`, `setTimeZone(zone)`, `loadMessages()`, `addResource(language, namespace, json)` ([I18n](services/i18n.md#tsx)) | `i18n` |
| `OperationService` | `dyn OperationService` | `instanceId`, `targets`, `onEvent`; as promises: `registerTarget`, `releaseTarget`, `begin`, `commit`, `abort`, `submitRecord`, `publishRemoteRecord`; with `OperationTarget` ([Operations](services/operations.md#tsx)) | `operations` |
| `HistoryService` | `dyn HistoryService` | `canUndo(scope)`, `canRedo(scope)`, `depths(scope)`, `onChange`; as promises: `bindTargetScope`, `undo`, `redo`, `clearScope`, `setScopeEntryLimit`, `retainedRecords` ([History](services/history.md#tsx)) | `history` |
| `SelectionService` | `dyn SelectionService` | `state()`, `activeArea()`, `status()`, `onChange`; as promises: `activate`, `publishStatus`, `deactivate`, `clear` ([Selection](services/selection.md#tsx)) | `selection` |
| `InputMapService` | `dyn InputMapService` | `current()`, `revision()`, `resolve(contexts, binding)`, `bindingsFor(action)`, `onChange`; as promises: `load()`, `applyUserDocument(expectedRevision, document)` ([Input map](services/input-map.md#tsx)) | `input_map` |
| `ApplicationConfigurationService` | `dyn ApplicationConfigurationService` | `current()`, `layoutDirection()`, `reducedMotion()`, `onChange`; as a promise: `replaceOptions(settings)` ([Application configuration](services/application-configuration.md#tsx)) | `application_configuration` |

```tsx
const title = signal("");

// With the provider `start()` gives, in its `.then(async (services) => { … })`:
async function welcome(services: ServiceProvider) {
  const i18n = services.getRequiredService(I18nService);
  title.set(i18n.t("welcome", { name: "Ada" }));
  const stop = i18n.onChange(() => title.set(i18n.t("welcome", { name: "Ada" })));

  await services.getRequiredService(KeyValueStore).setJson("settings", "theme", { mode: "dark" });
}
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
class Clock { now() { return Date.now(); } }
class Page { constructor(readonly clock: Clock) {} }
interface Greeter { greet(name: string): string; }
class EnglishGreeter implements Greeter { greet(name: string) { return `Hello, ${name}!`; } }

const GreeterToken = new ServiceToken<Greeter>("Greeter"); // the interface's token

new RayXAppBuilder()
  .addSingleton(Clock, () => new Clock())
  .addScoped(Page, (services) => new Page(services.getRequiredService(Clock)))
  .addSingletonAs(GreeterToken, () => new EnglishGreeter());
```

- Every Rust registration has its twin: `addSingleton`, `addScoped`, `addTransient`, their `As` forms, and the
  `addStartup…` forms ([Startup](startup.md#startup-services-in-ts)). A factory gets the resolving provider and may
  resolve TS and Rust services; one that throws fails the resolve with its message.
- A RayX service's class is the token of its contract: register a TS implementation of the service under it
  ([below](#implementing-a-service-in-ts)), and the app's own services under a TS class or a `ServiceToken`.
- **The engine thread.** TS services are made and resolved on the engine thread only. A TS service is made in a call
  from JS (a TS `getService`); startup services and contract singletons are made by `start()`. Rust resolving a TS service off
  the engine thread fails (`ServiceResolutionError::WrongThread`); resolving one that is not made yet, outside a call
  from JS, fails too. To Rust, a TS service is an opaque entry; Rust calls it only through a contract.

## Contracts

A contract is a Rust trait TS implements, which Rust resolves as `Arc<dyn Trait>`:

```rust
#[rayx::contract]
pub trait WeatherSource {
    /// The latest forecast, published by the implementation.
    #[state(ts = "string")]
    fn latest(&self) -> &StateCell<String>;
    /// Each alert as it comes.
    #[events(ts = "string")]
    fn alerts(&self) -> &EventStream<String>;
    /// The source's name, read once.
    #[constant]
    fn name(&self) -> String;
    /// The forecast for `city`.
    async fn forecast(&self, city: String) -> Result<String, WeatherError>;
    /// Notes `line`.
    fn note(&self, line: String);
}

// In the app's set: `registry.contract::<dyn WeatherSource>()`.
let source = services.required_service::<dyn WeatherSource>()?;
let now = source.latest().get();
let forecast = source.forecast("Rijeka".into()).await?;
```

```tsx
import { ContractEvents, ContractState, RayXAppBuilder, weatherSourceToken, type WeatherSource } from "my-app";

class OpenMeteo implements WeatherSource {
  latest = new ContractState("unknown");
  alerts = new ContractEvents<string>();
  name = "Open-Meteo";
  async forecast(city: string) {
    this.latest.publish(`sunny in ${city}`);
    return this.latest.value;
  }
  note(line: string) { console.log(line); }
}

new RayXAppBuilder().addSingletonAs(weatherSourceToken(), () => new OpenMeteo());
```

### Shapes

A contract's members are of five kinds, and TS implements each in its own way:

| Kind | Rust | TS implementation | Rust reads it |
| --- | --- | --- | --- |
| Published state | `#[state] fn m(&self) -> &StateCell<T>` | a field `m = new ContractState(initial)`, `m.publish(value)` on each change | `m().get()` and `m().subscribe(..)`, synchronously, from any thread |
| Published events | `#[events] fn m(&self) -> &EventStream<E>` | a field `m = new ContractEvents()`, `m.publish(event)` | `m().subscribe(..)`, each event once, on any thread; nothing is kept |
| Constant | `#[constant] fn m(&self) -> T` | a field (or getter) `m` | `m()`, read once |
| Command | `async fn m(&self, ..) -> Result<T, E>` | an `async` method; what it throws is the error | `m(..).await` |
| Posted command | `fn m(&self, ..)` returning nothing | a method | `m(..)`, not awaited |

- TS names are camelCase (`root_region_name` is `rootRegionName`). Commands' values cross as JS values (`String`,
  numbers, `#[derive(JsObject)]` structs); state, events and constants as JSON (`serde` types, their fields as serde
  names them), with `ts = "…"` on the attribute giving the TS type where the Rust type does not say it, and
  `#[ts("(..): ..")]` on a command its TS signature.
- A command's error type `E` is `From<ContractError> + From<Exception>` (`String` is one): what TS throws arrives as
  the `Exception`, and a call that never reached TS as a `ContractError`.
- `publish` throws, keeping the value, when the value is not what the contract's state (or events) hold.
- A contract has no generics, associated items, supertraits or default bodies; the trait gains `Send + Sync`.

### Threads and costs

- **Published state and constants** are linked once and then cost nothing to read: a singleton's (and every startup
  form's) in `start()`, before any Rust startup service's factory runs, so such a factory may read them; a scoped or
  transient implementation's when it is resolved on the engine thread, or on its first read when it was resolved on
  another thread, which then waits for the engine. So a factory that runs off the engine thread must not read a TS
  contract's state, events or constants: resolve the contract there and read it afterwards.
- **Commands** reach TS from any thread. On the engine thread a call goes straight to JS and its future resolves in
  JS's next turn; off it, the call goes through the surface's queue (`EngineRunner`), waits for the engine thread, and
  its answer comes back the same way, so it costs a hop to the engine thread and back and runs in turn with the UI's
  work. Calls from Rust code JS runs outside a call (a stop function) are queued the same way. Once the engine
  stopped, a call fails with `ContractError::EngineGone`.
- **Posted commands** run on the spot inside a call from JS and are posted, in order, otherwise.
- Rust startup services that depend on `dyn WeatherSource` initialize after it.

### The interface and the token

TS gets the interface `WeatherSource` (`#[rayx::contract(ts = "WeatherSourceContract")]` names it) and the token
function `weatherSourceToken()`, which the app adds to its set with `registry.contract::<dyn WeatherSource>()`
(`ContractRegistryExt`, in the prelude). Every registration form takes the token, and Rust resolves the
implementation with the same lifetime.

RayX's services are contracts too: each core trait's TS interface is `<Name>Contract` (`PlatformInfoServiceContract`,
`KeyValueStoreContract`, `I18nServiceContract`, …) and its token is the service's class, which the service crate's set registers
(`registry.contract_class::<..>()`).

## Implementing a service in TS

An app replaces RayX's implementation of a service with its own TS class: it implements the service's
`<Name>Contract` and registers under the service's class, in place of the builder method that registers RayX's.
Rust then resolves it as `dyn Trait`, RayX's other services use it, the host's requirement counts it, and
`getRequiredService(<Name>)` wraps it like RayX's:

```tsx
import {
  ContractState, PlatformInfoService, RayXAppBuilder, type PlatformInfoServiceContract, type PlatformSnapshot,
} from "my-app";

declare const snapshot: PlatformSnapshot; // the app's: the JSON of a PlatformSnapshot

class MyPlatform implements PlatformInfoServiceContract {
  platform = new ContractState(snapshot);
  isInitialized() { return true; }
  async initialize() {}
}

new RayXAppBuilder()
  .addStartupSingletonAs(PlatformInfoService, () => new MyPlatform())
  .addSuspension()
  .addAssets()
  .addFonts()
  .start()
  .then((services) => { /* RayX's PlatformInfoService class wraps MyPlatform */ });
```

Register it with a startup form (`isInitialized()` and `async initialize()`) when it must load before the services that
use it; RayX's startup services that depend on the trait wait for it. Each service's page shows its contract under
"Replace it".

Type the class for NSH's strict `tsc`: a class implementing an interface takes no types from it, so its methods'
parameters, its `ContractState<T>` and `ContractEvents<E>` and a method that only throws are typed with the set's
types (`PlatformSnapshot`, `NavigationRequest`, `Promise<NavigationResult>`, …). The TSX examples in these docs
type-check that way; a `declare` line in one stands for the app's own code, and an example that is only part of a
file says so.

## Options

TS options are a token with their defaults, configured as Rust's are (see
[Configuration and options](configuration.md#options-in-ts)):

```tsx
const Feed = new OptionsToken("Feed", { title: "News", pageSize: 10 });

new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addOptions(Feed, (options) => options
    .bind("Feed")
    .validate((feed) => feed.pageSize > 0, "pageSize must be positive")
    .validateOnStart())
  .configureOptions(Feed, (feed) => ({ ...feed, title: feed.title.trim() }))
  .start()
  .then((services) => {
    const feed = services.getOptions(Feed).value;
  });
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

new RayXAppBuilder()
  .addPlatform().addSuspension().addAssets().addFonts()
  .addStorage()
  .addWeather()
  .start()
  .then((services) => { /* the weather service resolves here */ });
```

The method runs the Rust function on the builder and returns the builder, and throws once the builder started.
`apply` takes a TS function of the builder, and runs it in chain order.

## View models

A view-model class exposes `Observable`s and `ReactiveCommand`s; see [View models](view-models.md#binding-in-tsx).

## Components

RayX's components (`Button`) and its theme document are RayX's library set, `rayx/components`, which the set
`#[rayx::native_module]` makes serves; import them from `rayx/components` in every app. See
[Components and theme](components.md) and, for a component's `*.meta.tsx` file, [Meta files](component-meta.md).

```tsx
import { Button } from "rayx/components";

declare function save(): void; // the app's own

const saveButton = <Button text="Save changes" variant="primary" onPress={save} />;
```
