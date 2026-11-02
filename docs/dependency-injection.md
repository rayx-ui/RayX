# Dependency injection

`rayx_core` has a `ServiceProvider` with the API of .NET's `Microsoft.Extensions.DependencyInjection`: singleton,
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
fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .add_singleton(ApiClient { base_url: "https://api.example.com".into() })
        .add_scoped(|services| EditorState::new(services.required_service::<ApiClient>().unwrap()))
        .add_transient(|_| Draft::default())
}
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

In a Rust component, `rayx_app::services()` returns the provider of the nearest screen scope (or the surface's root).

## Lifetimes

| Lifetime | Core | On NSH |
| --- | --- | --- |
| Singleton | one per root provider, shared by its scopes | one per engine surface: each surface builds its root from the function `set_app` names, when the app starts there |
| Scoped | one per scope, dropped with it | one per screen: `use_screen_scope()` in a Rust component, dropped on the component's cleanup; `app.scope()` in TSX, `scope.dispose()` in its cleanup |
| Transient | one per resolve | the same |

## Lifetimes on NSH

NSH has no process-wide singletons: each window, test and hot restart is a surface of its own. So `rayx_app` keeps one
root provider per surface, built from the function `set_app` names.

```rust
// A Rust screen: a child scope of the enclosing screen's, provided to what the component renders.
let scope = use_screen_scope()?;
```

```tsx
// TSX: a scope per screen, from mount to cleanup.
const scope = app.scope();
// ...
scope.dispose();
```

## View models

`use_view_model::<T>()` resolves an `ActivatableViewModel` in the screen's scope and keeps it active until the
component's cleanup. `ServiceScope::view_model::<T>()` does the same for a TSX screen until its scope is disposed.

```rust
let vm = use_view_model::<CounterViewModel>()?; // Arc<CounterViewModel>, active with the screen
```

How a view model's state reaches the UI is in [View models](view-models.md).
