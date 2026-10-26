# RayX

RayX is an app framework for [Native Shell (NSH)](https://github.com/dXnSix/nsh) apps; it replaces the GPUI and gpux
stack of the previous RayX (now `RayX.Old`). It starts with dependency injection, options and an app builder with the
API of RayX's `rayx_runtime::di`, which until `v0.3.0` were the `nsh-app` repository's `nsh_app*` crates. They come in
two layers:

- `rayx_core`: the pure-Rust core: a `ServiceProvider` with singleton, scoped and transient services, scopes,
  options and the `AppBuilder`. No NSH dependency, so service crates use it and stay testable with `cargo test` and
  buildable for every target, wasm32 included.
- `rayx_app`: the NSH layer: one root provider per NSH surface, a scope per screen (created on mount, dropped on
  cleanup), view-model activation on mount ([`dxn_reactive`](https://github.com/dXnSix/dXn.Reactive)'s
  activators), and access from `#[class]` bridges and Rust components.

`rayx_app_macros` holds `#[derive(AppService)]`, which both layers re-export.

## Installation

Releases are git tags `vX.Y.Z`; nothing is published to a registry. Depend on a tag:

```toml
[dependencies]
rayx_core = { git = "https://github.com/rayx-ui/RayX", tag = "v0.4.0" } # service crates
rayx_app = { git = "https://github.com/rayx-ui/RayX", tag = "v0.4.0" }    # NSH bridge crates
```

Coming from `v0.4`: `rayx_app_di` is `rayx_core` now; rename the dependency and its paths. Coming from `nsh-app`
`v0.3.0`: rename the dependencies (`nsh_app` to `rayx_app`, `nsh_app_di` to `rayx_core`) and their paths, and move to
`dxn_reactive` (formerly `rayx_reactive`), whose activators `rayx_app` re-exports.

## Startup

Services can initialize asynchronously: register them with `add_startup_singleton` (or `_scoped`, `_transient`, the
`_as` forms) and implement `AsyncInitializable` (`initialize`) and `StartupInitializable` (`initialization_depends_on`).
Starting the app builds the root provider and initializes every startup service in dependency order, a layer's
services concurrently. The app is ready, and services resolve, only once all of them initialized.

- **Failure:** one failing service fails startup, with the services that failed and why; nothing is installed.
- **Timeout:** each service may set `initialization_timeout()`; one that sets none gets the builder's default
  (`default_initialization_timeout(..)`, 30 s unless set). A service past its timeout fails startup (`timed_out`).

```tsx
// TSX: register `AppServices` and `ServiceScope` in the app's UI set, then
const app = await AppServices.load(); // once: starts the app; rejects if a service failed or timed out
const scope = app.scope();            // on a screen's mount; Rust bridges take it as Instance<ServiceScope>
scope.dispose();                      // in its cleanup
```

```rust
// A Rust root: start the app and show a splash until it is ready.
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(|| match rayx_app::app_state() {
        AppState::Ready => app().into_element(),
        AppState::Failed(message) => error_screen(message).into_element(),
        _ => splash().into_element(),
    })
}
```

`AppServices.load()` is asynchronous because startup awaits the services; it runs as an NSH task, where NSH's timers
and the surface's slot are reachable (a class's synchronous calls run outside the reactive runtime). Everything after
it is synchronous.

## Lifetimes on NSH

| Lifetime | Core | On NSH |
| --- | --- | --- |
| Singleton | one per root provider, shared by its scopes | one per engine surface: `rayx_app::set_app(build)` names the builder function, each surface builds its root from it when the app starts there |
| Scoped | one per scope, dropped with it | one per screen: `use_screen_scope()` in a Rust component (dropped on the component's cleanup); `app.scope()` in TSX, `scope.dispose()` in its cleanup |
| Transient | one per resolve | the same |

`use_view_model::<T>()` resolves an `ActivatableViewModel` in the screen's scope and keeps it active until the
component's cleanup; `ServiceScope::view_model::<T>()` does the same for TSX until the scope is disposed.

NSH comes from the integration tag of the fork Gamify builds with (`[patch.crates-io]` in `Cargo.toml`); a consumer's
own patch decides which NSH it builds with.

Status: built for Gamify (github.com/dXnSix), consumed through git tags (`v0.4.0`, on NSH 0.2.0).
