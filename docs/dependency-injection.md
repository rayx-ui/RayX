# Dependency injection

`rayx_app` has a `ServiceProvider` with the API of .NET's `Microsoft.Extensions.DependencyInjection`: singleton,
scoped and transient services, child scopes, and services that initialize asynchronously at startup
([Startup](startup.md)).

## Services

A service is any `Send + Sync + 'static` type. `#[derive(AppService)]` implements the marker trait `AppService` for
it, through whichever RayX crate the calling crate depends on:

```rust
use rayx::prelude::*;

#[derive(AppService)]
struct ApiClient {
    base_url: String,
}

#[derive(AppService)]
struct Cache<T: Send + Sync + 'static> {
    entries: Vec<T>,
}
```

## Registering

Register services on the app builder, in the app's composition root:

```rust
RayXAppBuilder::new()
    .add_singleton(ApiClient { base_url: "https://api.example.com".into() })
    .add_scoped(|services| EditorState::new(services.required_service::<ApiClient>().unwrap()))
    .add_transient(|_| Draft::default())
    .start();
```

TSX registers its own services, TS classes, in the same container ([TSX](tsx.md#services-written-in-ts)):

```tsx
new RayXAppBuilder()
  .addSingleton(ApiClient, () => new ApiClient("https://api.example.com"))
  .addScoped(EditorState, (services) => new EditorState(services.getRequiredService(ApiClient)))
  .addTransient(Draft, () => new Draft());
```

- `add_singleton`, `add_scoped`, `add_transient` register a type; the `_as` forms (`add_singleton_as::<dyn Trait>(..)`)
  register it as a trait object.
- `add_startup_singleton`, `add_startup_scoped`, `add_startup_transient` (and their `_as` forms) register a service
  that initializes asynchronously before the app is ready.
- Service crates add their own builder extensions on the `AppBuilder` trait (`add_history()`,
  `add_navigation(..)`, …), so an app registers a whole feature in one call.

## Resolving

```rust
let services = builder.build()?;
let api = services.required_service::<ApiClient>()?; // Arc<ApiClient>, or an error
let maybe = services.service::<Cache<u32>>();         // Option<Arc<Cache<u32>>>
let scope = services.create_scope();                  // a child scope
```

In a Rust component, `rayx_app_nsh::services()` returns the provider of the nearest screen scope (or the surface's root).
In TSX, `await builder.start()` gives the surface's root:

```tsx
const api = services.getRequiredService(ApiClient); // the service, or an Error naming the token
const maybe = services.getService(Cache);           // the service, or null
const scope = services.createScope();               // a child scope; `scope.dispose()` drops it
```

## Runtime keys

A service that is not a Rust type (a service written in TypeScript, for one) is registered under a `RuntimeKey`, an
opaque key with a display name. It follows the same lifetimes, scopes and disposal as a typed service; its value is
type-erased (`ErasedService`, an `Arc<dyn Any + Send + Sync>`), and its factory may fail:

```rust
let settings = RuntimeKey::new("Settings"); // a new key per call, even for a name used before
services.add_singleton_keyed(settings.clone(), |services| {
    let api = services.required_service::<ApiClient>()?;
    Ok(Arc::new(Settings::new(api)) as ErasedService)
});
let value = services.required_service_by_key(&settings)?; // or `service_by_key`, an Option
```

A missing key fails with `ServiceResolutionError::MissingKey`, naming the key's display name. RayX registers each TS
service under a runtime key named after its class or `ServiceToken`, thread-bound (below).

## Thread-bound services

A value that is neither `Send` nor `Sync` is registered thread-bound (`add_singleton_thread_bound`,
`add_scoped_thread_bound`, `add_transient_thread_bound`, and the `_keyed` forms). It is made on the thread that first
resolves it and resolves there only, as an `Rc` (`required_thread_bound_service::<T>()`,
`required_thread_bound_service_by_key(&key)`); on any other thread resolving fails with
`ServiceResolutionError::WrongThread`, without touching the value. Resolving it through the shared API (`service`,
`service_by_key`), or a shared service through the thread-bound one, fails without making anything. The provider itself
stays usable from every thread.
A thread-bound value is never dropped on another thread: released elsewhere, it is dropped on its own thread at that
thread's next thread-bound resolve, or by `release_orphaned_thread_bound_services()`.

TS services are thread-bound to the engine thread: they are made and resolved there only. One is made in a call from
JS (a TS `getService`); startup and contract services are made by `start()`. Rust resolving a TS service off the
engine thread fails with `WrongThread`; resolving one that is not made yet, outside a call from JS, fails too.

## Contracts

Rust uses a TS service only through a contract: a Rust trait marked `#[rayx::contract]`, which TS implements and Rust
resolves as `Arc<dyn Trait>`, like any trait-object service. Its `async fn`s are awaited in JS's next turn, and a call
off the engine thread fails with `ContractError::WrongThread`:

```rust
let source = services.required_service::<dyn WeatherSource>()?;
let forecast = source.forecast("Rijeka".into()).await?;
```

```tsx
builder.addSingletonAs(weatherSourceToken(), () => new OpenMeteo());
```

See [TSX](tsx.md#contracts).

## Lifetimes

| Lifetime | Core | On NSH |
| --- | --- | --- |
| Singleton | one per root provider, shared by its scopes | one per engine surface: each surface builds its root from the builder the app starts there |
| Scoped | one per scope, dropped with it | one per screen: `use_screen_scope()` in a Rust component, dropped on the component's cleanup; `services.createScope()` in TSX, `scope.dispose()` in its cleanup |
| Transient | one per resolve | the same |

## Lifetimes on NSH

NSH has no process-wide singletons: each window, test and hot restart is a surface of its own. So `rayx_app_nsh` keeps one
root provider per surface, built from the builder the app starts there (`RayXAppBuilder::new()….start()` in a Rust
root, `await builder.start()` in TSX).

```rust
// A Rust screen: a child scope of the enclosing screen's, provided to what the component renders.
let scope = use_screen_scope()?;
```

```tsx
// TSX: a scope per screen, from mount to cleanup.
const scope = services.createScope();
// ...
scope.dispose();
```

## View models

`use_view_model::<T>()` resolves an `ActivatableViewModel` in the screen's scope and keeps it active until the
component's cleanup. `ServiceProvider::view_model::<T>()` (`rayx_app_nsh::ts`) does the same, in a Rust bridge, for a TSX
screen until its scope is disposed.

```rust
let vm = use_view_model::<CounterViewModel>()?; // Arc<CounterViewModel>, active with the screen
```

How a view model's state reaches the UI is in [View models](view-models.md).
