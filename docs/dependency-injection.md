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
    .add_platform()
    .add_suspension()
    .add_assets(|_| {})
    .add_fonts(|_| {})
    .add_singleton(ApiClient { base_url: "https://api.example.com".into() })
    .add_scoped(|services| EditorState::new(services.required_service::<ApiClient>().unwrap()))
    .add_transient(|_| Draft::default())
    .start();
```

TSX registers its own services, TS classes, in the same container ([TSX](tsx.md#services-written-in-ts)):

```tsx
class ApiClient { constructor(readonly baseUrl: string) {} }
class EditorState { constructor(readonly api: ApiClient) {} }
class Draft { text = ""; }

new RayXAppBuilder()
  .addSingleton(ApiClient, () => new ApiClient("https://api.example.com"))
  .addScoped(EditorState, (services) => new EditorState(services.getRequiredService(ApiClient)))
  .addTransient(Draft, () => new Draft());
```

- `add_singleton`, `add_scoped`, `add_transient` register a type; the `_as` forms (`add_singleton_as::<dyn Trait>(..)`)
  register it as a trait object.
- `add_startup_singleton`, `add_startup_scoped`, `add_startup_transient` (and their `_as` forms) register a service
  that initializes asynchronously before the app is ready; `add_startup_singleton_for::<dyn Trait, Impl>(..)` (and its
  factory, scoped and transient forms) registers an implementation of a trait, a contract say, as the trait and as a
  startup service registered as it ([Startup](startup.md#implementing-a-trait)).
- The last registration of a key wins completely: registering a service under a key removes the startup services
  registered as that key before (unless it registers its own), so the app's own `dyn Trait`, registered after a
  service crate's `add_x`, keeps RayX's from starting.
- Service crates add their own builder extensions on the `AppBuilder` trait (`add_history()`,
  `add_navigation(..)`, …), so an app registers a whole feature in one call. Each registers its service as the app's
  `dyn Trait`, replacing any registered before; an app that wants its own implementation registers it instead
  ([App services](services.md#replacing-a-service)).

## Required services and declared dependencies

What an app needs, and what each registration needs, is declared rather than registered, and checked before anything
is made:

- `services.require::<dyn T>(hint)`: the app (or its host) needs `T`. RayX's NSH host requires
  `dyn PlatformInfoService`, `dyn SuspensionService`, `dyn AssetService` and `dyn FontService` at the start
  ([Startup](startup.md#how-the-app-starts)).
- `services.require_for::<dyn S, dyn T>(hint)`: the registration of `S` resolves `T`. A service crate's builder
  method declares this for every service its implementation resolves (`add_fonts` declares
  `dyn FontService` on `dyn AssetService`), so the app's own `T`, registered before or after it, satisfies it as well
  as RayX's. A registration's requirements belong to it: declared before registering `S`, they are the next
  registration of `S`'s (the current one's when none follows), and a later registration of `S` that does not declare
  them again drops them, so the app's own `dyn FontService` registered after `add_fonts` needs no asset service.
  What the app or its host requires (`require`) stays.
- `services.validate()`, which `RayXAppBuilder::build` and `build_async` run first: one `ServiceValidationError`
  listing every problem (`ServiceValidationProblem`): `Missing` (the service, who needs it, the hint saying how to
  register it) and `SingletonOnScoped` (a singleton that depends on a scoped service, directly or through transient
  ones). On NSH the start fails with `AppError::Services` before any source loads or any factory runs.

```text
the app's services are not complete:
- `dyn rayx_app::services::fonts::FontService` needs `dyn rayx_app::services::assets::AssetService`, which is not
  registered: add_assets(..) (addAssets in TS) from rayx_services_assets, or register your own dyn AssetService
```

A service's own crate writes the hint; an app's own extension declares its dependencies the same way:

```rust
builder.services_mut().require_for::<WeatherService, dyn KeyValueStore>("add_storage() from rayx_services_storage");
```

## Resolving

```rust
let services = builder.build()?;
let api = services.required_service::<ApiClient>()?; // Arc<ApiClient>, or an error
let maybe = services.service::<Cache<u32>>();         // Option<Arc<Cache<u32>>>
let scope = services.create_scope();                  // a child scope
```

In a Rust component, `rayx_app_nsh::services()` returns the provider of the nearest screen scope (or the surface's root);
in a TSX component's body, `useServices()` does the same ([screens in TSX](#screens-in-tsx)).
`await builder.start()` gives the surface's root (a fragment: `services` is that root, `ApiClient` and `Cache`
the app's classes):

<!-- tsx: fragment -->
```tsx
const api = services.getRequiredService(ApiClient); // the service, or an Error naming the token
const maybe = services.getService(Cache);           // the service, or null
const scope = services.createScope();               // a child scope; `scope.dispose()` drops it
```

## Scopes with registrations of their own

`create_scope_with` (TS: `createScope(configure)`) makes a child scope with registrations only it and its descendants
see: singleton, scoped and transient services and their `_as` forms, by type, runtime key or (in TS) token or contract.
What it is for: a fake or a different implementation for one part of an app (a preview, a test) without a second root.

```rust
let scope = services.create_scope_with(|scope| {
    scope.add_singleton_as::<dyn Clock>(Arc::new(FixedClock(7)) as Arc<dyn Clock>);
});
```

<!-- tsx: fragment -->
```tsx
const scope = services.createScope((scope) => {
  scope.addSingletonAs(Clock, () => new FixedClock(7));
});
```

- For a key it registers, its registration wins in it and its descendants over the root's (and a nearer scope's over
  an outer one's), also when the root made that key already. The provider it was made from, that provider's other
  scopes and the root resolve as before.
- A singleton it registers is one instance per scope made this way, shared by its descendants (a grandchild resolves
  the same one), and always made with that scope: once the scope is gone (dropped, or disposed in TS), a descendant
  that resolves it first gets one made over the scope's registrations and a scoped cache of its own, never over the
  descendant's scoped services or replacements, and every other descendant shares it. It is dropped when the scope
  and all its descendants are gone.
- Scoped and transient services, its own and the root's, resolve their dependencies in the provider they are resolved
  from: a root-registered scoped service resolved in the scope sees the scope's replacements.
- A root-registered singleton is always made with the root, whichever scope resolves it first, so it never captures a
  scope's replacements or scoped instances.
- A registration whose dependency is missing fails the resolve with the error a root registration gets.
- Startup services belong to the root: `ScopeRegistrations` has no startup forms in Rust, and TS's throw, naming the
  token.

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

TS services are thread-bound to the engine thread: they are made and resolved there only. One is made where JS runs
on the spot: in a call from JS (a TS `getService`), or on the engine thread between JS's turns (a Rust task or
factory); startup and contract services are made by `start()`. Rust resolving a TS service off the engine thread
fails with `WrongThread` (a contract's adapter reaches it through the engine instead).

## Contracts

Rust uses a TS service only through a contract: a Rust trait marked `#[rayx::contract]`, which TS implements and Rust
resolves as `Arc<dyn Trait>`, like any trait-object service. Its `async fn`s are awaited in JS's next turn, from any
thread (a call off the engine thread goes through the surface's queue, and fails with `ContractError::EngineGone` once
the engine stopped):

```rust
let source = services.required_service::<dyn WeatherSource>()?;
let forecast = source.forecast("Rijeka".into()).await?;
```

<!-- tsx: fragment -->
```tsx
builder.addSingletonAs(weatherSourceToken(), () => new OpenMeteo());
```

See [TSX](tsx.md#contracts), which has `OpenMeteo`.

## Lifetimes

| Lifetime | Core | On NSH |
| --- | --- | --- |
| Singleton | one per root provider, shared by its scopes | one per engine surface: each surface builds its root from the builder the app starts there |
| Scoped | one per scope, dropped with it | one per screen: `use_screen_scope()` in a Rust component, `useScreenScope()` in a TSX one, dropped on the component's cleanup |
| Transient | one per resolve | the same |

## Lifetimes on NSH

NSH has no process-wide singletons: each window, test and hot restart is a surface of its own. So `rayx_app_nsh` keeps one
root provider per surface, built from the builder the app starts there (`RayXAppBuilder::new()….start()` in a Rust
root, `await builder.start()` in TSX). A design tool that reloads the app's composition starts it afresh with
`builder.restart()` (Rust `restart_async()`): the old root goes, and in TSX the old root and the scopes made from it
are disposed ([TSX](tsx.md)).

```rust
// A Rust screen: a child scope of the enclosing screen's, provided to what the component renders.
let scope = use_screen_scope()?;
```

### Screens in TSX

`useScreenScope()`, in a TSX component's body, makes the component a screen: a child scope of `useServices()`,
provided to everything it renders, TSX and Rust components alike (a Rust component's `services()` under it returns
that scope), and disposed at the component's cleanup with the scoped services it resolved and the view models
activated through it. A screen inside a screen gets a child of the outer one's scope. `useServices()` gives every
component under one screen the same `ServiceProvider` object (so a service's class object is the same too), and the
surface's root is one object per surface:

<!-- tsx: fragment -->
```tsx
function Screen(props) {
  const screen = useScreenScope();
  pages[`${props.name} screen`] = Pages.pageOf(screen);
```

<!-- tsx: fragment -->
```tsx
function Under(props) {
  const services = useServices();
```

The hooks run their Rust code under the Rust owner NSH ties to the calling component's JS owner
(`CallCx::with_owner`). In NSH's TSX a component's body has no reactive owner of its own (as in Solid), so a screen's
scope belongs to the JS owner it was made under: **screens under owners of their own are isolated** (an `<If>` or
`<Each>` branch, a provider, a navigation region's page), and **views placed statically side by side in one component
share their owner's scope**. Put each screen in a branch of its own:

<!-- tsx: fragment -->
```tsx
      <If when={open()}>
        <Screen name="first" nested={true} />
      </If>
      <If when={open()}>
        <Screen name="second" nested={false} />
      </If>
```

A TSX component in a Rust screen's children (`use_screen_scope()` in its view) sees that screen's scope when the
children are lazy (`#[prop(lazy)]`: made once the screen places them); eager children are made before the screen's
view and see the scope around it.

`provideServices(provider)` provides a provider made elsewhere (a scope a test or a design tool made) to what the
component renders, as a screen's scope is provided, without disposing it at the component's cleanup (Rust:
`provide_services(provider)`):

<!-- tsx: fragment -->
```tsx
function Provided() {
  provided = root.createScope();
  provideServices(provided);
  return <ProvidedChild />;
}
```

Called before RayX started on the surface the hooks throw saying it is starting, or has not started (the words of
`AppError::Starting` and `AppError::NotStarted`); outside a component's body (no reactive owner) they throw saying so.
`appState()`, the twin of `app_state()`, is an NSH signal of where the app is (`{ state: "notStarted" | "starting" |
"ready" }` or `{ state: "failed"; message }`), the same signal every time, following the app's start on its surface
(and a `restart()`, which takes it through `"starting"` again);
mount the components bound to view models once it is `"ready"`:

<!-- tsx: fragment -->
```tsx
const state = appState();
const seen = [];
effect(() => {
  const now = state();
  seen.push(now.state === "failed" ? `failed: ${now.message}` : now.state);
});
```

## View models

`use_view_model::<T>()` resolves an `ActivatableViewModel` in the screen's scope and keeps it active until the
component's cleanup; `useViewModel(token)` is its TSX twin. `ServiceProvider::view_model::<T>()` (`rayx_app_nsh::ts`)
activates one in a Rust bridge until the provider is disposed.

```rust
let vm = use_view_model::<CounterViewModel>()?; // Arc<CounterViewModel>, active with the screen
```

How a view model's state reaches the UI is in [View models](view-models.md).
