---
agentx:
  kind: state
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-16T23:00:00+01:00
  topics:
    - rayx_app
    - nsh
    - tsx
    - rayx
  supersedes: []
  superseded_by: []
---
# app: state

## Purpose And Usage

Route work on the NSH-facing layer and the umbrella.

## Current Technical Shape

`rayx_app` holds the app's start and a root provider per engine surface (`surface.rs`: a Rust
root builds `RayXAppBuilder::new()` with its chain and calls `StartAppBuilderExt::start()`,
watched through `app_state()`, or awaits `start_async()`; there is no `set_app`), a scope per
screen (`screen.rs`), view-model binding (`binding.rs`), navigation regions and the location
(`region.rs`, `location.rs`), the dev tools (`devtools/`) and the NSH side of the services
(`settings.rs`, `storage.rs`, `platform.rs`, `suspension.rs`). With `rayx_components`
([components](../components/state.md)) it is one of the two crates that depend on NSH.

Its TS side is `src/ts/` (epic `ts-app-builder`), registered whole by `ts::register`:

- `builder.rs`: TS's `RayXAppBuilder`, a camelCase twin of every builder method delegating to
  the Rust extension of its name (options callbacks applied after binding, timeouts in ms),
  `apply(extension)` and `start()`, which resolves with the surface's root `ServiceProvider`;
  its other methods throw once it started.
- `provider.rs`: `ServiceProvider` (`getService`, `getRequiredService`, `getOptions`,
  `createScope`, `dispose`; Rust bridges take `Instance<ServiceProvider>`, `view_model` activates
  until dispose), `ServiceToken<T>` and the alias `ServiceKey<T>`; a RayX service's class is its
  own token.
- `container.rs`: TS services as thread-bound entries under runtime keys (engine thread only;
  made in a call from JS, startup and contract services by `start()`); `startup.rs` the TS
  `AsyncInitializable`/`StartupInitializable` interfaces in the same startup graph; `ready.rs`
  `onReady`/`onReadyWhen`; `app_options.rs` `OptionsToken`/`OptionsBuilder` (`addOptions`,
  `configureOptions`).
- `contract.rs`: `#[contract]` traits (TS interface and `<trait>Token()`, resolved by Rust as
  `Arc<dyn Trait>`, `ContractError::WrongThread` off the engine thread) and
  `registry.contract::<dyn Trait>()`. The token is a generated function per contract, the
  trait's name in camel case plus `Token` (`weatherSourceToken()`), as the owner chose.
- `services.rs`, `i18n.rs`, `region.rs`: the services' classes under the Rust names
  (`I18nService`, `NavigationService`, `KeyValueStore`, `ApplicationConfigurationService`,
  `AssetService`, `FontService`, `PlatformInfoService`, `SuspensionService`,
  `NavigationRegion`); each listener returns a function that stops it.

`rayx_app_macros` has `#[derive(AppService)]`, `#[app_extension]` (a Rust builder extension and
its flat TS module `addX`), `#[contract]` and `#[native_module]`. `rayx` re-exports `rayx_core`,
`rayx_runtime`, `rayx_app`, `rayx_components` (`rayx::components`, not in the prelude),
`dxn_i18n` and `dxn_intl` with a prelude (`Registry` among it), and the attribute
`#[rayx::native_module("spec")]` on an app's `fn native(registry: &mut Registry)`: NSH's
`#[native_module]` on it, whose body first calls `rayx::app::register` and
`rayx::components::register`. Its line in the crate's source is what NSH's CLI reads the set's
specifier from (`specifier_of`), which the web runtime keeps the set's gate by; a function-like
macro would hide it, so there is none. The `rayx new` templates and the gallery's crate use it
([cli](../cli/state.md)).

## Open follow-ups

- `rayx_app::ts` reads whether a TS factory may run on the spot from NSH's `#[doc(hidden)]`
  `JsHandle::handle().in_call()` (`ts/container.rs`): ask the fork to make it public API.
- The TS wrapper objects of Rust services are per TS `ServiceProvider` (`===` holds within one
  provider, not across scopes), unlike TS singletons, which are one object per root.

## Backlinks

Code and docs: [docs/tsx.md](../../../../docs/tsx.md),
[docs/startup.md](../../../../docs/startup.md),
[docs/view-models.md](../../../../docs/view-models.md),
[docs/navigation.md](../../../../docs/navigation.md); tests `crates/rayx_app/tests/nsh_*.rs`
(`nsh_tsx_*` for the TS side, `nsh_app_start` for a Rust root), `crates/rayx/tests/umbrella.rs`,
`crates/rayx/tests/native_module.rs`, `crates/rayx/tests/native_module_attribute.rs`.

Memory: [dependencies](../dependencies/state.md) (the NSH pin), [runtime](../runtime/state.md).
