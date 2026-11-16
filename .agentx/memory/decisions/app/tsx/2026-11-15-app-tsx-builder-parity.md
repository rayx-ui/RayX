---
agentx:
  kind: decision
  scope: decisions/app/tsx
  status: current
  created: 2026-11-15T18:45:00+01:00
  updated: 2026-11-16T12:00:00+01:00
  topics:
    - tsx
    - app-builder
    - dependency-injection
    - startup
    - nsh
  supersedes: []
  superseded_by: []
---
# Decision: one app builder API in Rust and TypeScript

**Date:** 2026-11-15, decided with the owner. **Status:** current; the epic `ts-app-builder`
(`.agentx/tasks/ts-app-builder/`) implements it. Until it lands, source still has the API it
replaces (`set_app`, `AppServices.load()`, `X.of(app)`).

## Context

A TSX app could not compose itself: its Rust crate named the services with `set_app(build_app)`
and registered each RayX class in its `#[native_module]`, and TSX only called
`AppServices.load()` and `X.of(app)`. Apps should be written the same way in either language.
NSH serves only native (Rust) modules, `nsh.json` `"rust"` lists crate folders, and JS and Rust
exchange JSON-like values, class instances and callbacks, but no other JS object.

## Decision

- **The same builder on both sides.** TS has `RayXAppBuilder` with a camelCase twin of every
  builder method (`addAppSettings`, `addStorage`, `addHistory`, `addI18n(options => …)`,
  `configureAppConfiguration`, `defaultInitializationTimeout`, `onReady`, the singleton, scoped,
  transient, startup and `As` registrations) and `await builder.start()`. Options objects are
  classes with Rust's methods; what TS sets applies after configuration binds, as in Rust.
- **Rust drops `set_app`.** A Rust root builds the same chain and calls `.start()`; each surface
  still builds its own provider; `app_state()` stays.
- **Resolution has Rust's names.** TS `ServiceProvider`: `getService`, `getRequiredService`,
  `createScope`, `dispose`. A Rust service's TS class is named after the Rust service
  (`I18nService`, `NavigationService`, `KeyValueStore`, …) and is its own token; `AppServices`,
  `ServiceScope` and `X.of(app)` go. Results are typed in TS, as in Rust:
  `getRequiredService<T>(token: ServiceKey<T>): T`, where `ServiceKey<T>` is
  `{ readonly prototype: T } | ServiceToken<T>`, so no call site casts.
- **One container.** Services written in TS live in the same `ServiceProvider`: same lifetimes and
  scopes, same startup graph, timeout and failure report; the app is ready only when Rust and TS
  startup services all initialized. TS services resolve on the engine thread only.
- **Rust uses a TS service only through a contract**: a Rust trait marked implementable from TS
  (`#[rayx::contract]`), resolved as `Arc<dyn Trait>`, whose calls into JS are async. The macro
  keeps the trait `dyn`-compatible and holds NSH's stand-in on the engine thread. A TS-only
  service is an opaque entry to Rust.
- **Startup interfaces mirror the Rust traits.** TS `AsyncInitializable` (`isInitialized`,
  `initialize(context): Promise<void>`, optional `initializationTimeout` in ms) and
  `StartupInitializable` (optional `startupServiceType`, optional `initializationDependsOn`
  naming TS and Rust services), with `StartupInitializationState` and
  `StartupInitializationContext`. `isInitialized` stays required in both languages;
  `startupServiceType` becomes optional in both, defaulting to the key a service is registered
  under. `startupServiceType`, `initializationDependsOn` and `initializationTimeout` are read once,
  when the service is made; `isInitialized` is asked just before initializing, as in Rust. A timeout
  fails startup but cannot stop JS work. In Rust these members are `async fn`s, since NSH calls a
  TS object's other members only inside a synchronous call from JS and the startup run is a task;
  NSH's typed signatures keep the TS shapes above. `StartupInitializable` repeats
  `AsyncInitializable`'s members (NSH interfaces have no supertraits).
- **A TSX app keeps a one-line Rust crate**: `rayx::native_module!("my-app")`; the app's own Rust
  services reach TS as `#[rayx::app_extension]` functions passed to `builder.apply(..)`.
- **NSH first.** The fork gains JS objects Rust keeps (`JsHandle`: identity, traced, engine
  thread only), `#[interface]` on a Rust trait (a TS `export interface` and the stand-in
  `Js<Trait>` calling the JS object) and a class constructor as an argument (`Constructor`), in
  [dXnSix/nsh#262](https://github.com/dXnSix/nsh/pull/262) (merged 2026-11-15 as `1666a956`); and
  typed TS signatures for class and interface members, approved by the owner the same day and
  merged as [dXnSix/nsh#265](https://github.com/dXnSix/nsh/pull/265) (`ea1a7ef3a`): `#[ts("…")]`,
  generic classes (`#[class(generic = "T")]`) and shared type aliases (`Registry::ts_alias`). Both
  ship in `nshx-0.5.0-4` (released 2026-11-16), which RayX pins once its theme is on NSH's theme
  documents: [dXnSix/nsh#264](https://github.com/dXnSix/nsh/pull/264), in the same tag, removes the
  theme types `rayx_components` is built on.

## Alternatives considered

- **A TS-side container**: NSH cannot ship TS from a Rust crate, and two containers would split
  the startup graph, scopes and failure report.
- **A builder for RayX's own services only**, lookup staying `X.of(scope)`: not the same API, and
  TS services stay outside DI.
- **No Rust crate for TSX apps**: needs `nsh.json` to name a dependency's set, a second NSH
  change; not now.

## Trade-offs accepted

- A breaking 0.x minor release, with `Migration: apps: …` footers.
- RayX's TSX side waits for the NSH tag carrying both features (`nshx-0.5.0-4`) and, through it,
  for RayX's theme to move onto NSH's theme documents.
- TS services are engine-thread only, and Rust reaches them asynchronously and only through
  contracts.

## Supersedes

None. It replaces the TSX shape documented in [docs/tsx.md](../../../../../docs/tsx.md) and
[docs/startup.md](../../../../../docs/startup.md) once the epic lands.
