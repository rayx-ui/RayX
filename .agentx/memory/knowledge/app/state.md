---
agentx:
  kind: state
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-22T12:00:00+01:00
  topics:
    - rayx_app_nsh
    - nsh
    - tsx
    - contracts
    - rayx
  supersedes: []
  superseded_by: []
---
# app: state

## Purpose And Usage

Route work on the NSH host (`rayx_app_nsh`) and the umbrella (`rayx`).

## Current Technical Shape

`rayx_app_nsh` runs the core on NSH, by topic folder:

- `app/`: `surface.rs` (a root provider per engine surface; a Rust root builds
  `RayXAppBuilder::new()` and calls `.start()`, watched through `app_state()`, or awaits
  `.start_async()`), `builder_ts.rs` (TS's `RayXAppBuilder`: only the core's methods, settings,
  configuration, options, DI registrations, ready hooks, `apply`, `start`, plus `extend_with`,
  which service crates' and `#[app_extension]`'s class-extension methods act through),
  `ready_ts.rs`, `state_ts.rs` (TS `appState()`, a signal per VM following `app_state()`, made
  with NSH's `CallCx::signal`). The start requires `dyn PlatformInfoService`, `dyn SuspensionService`,
  `dyn AssetService` and `dyn FontService` (`require_host_services`, hints in the start's
  language); nothing is pre-registered.
- `di/`: `screen.rs` (a scope per screen, one TS `ServiceProvider` object per scope, Rust
  `provide_services`), `screen_ts.rs` (TS `useServices`, `useScreenScope`, `provideServices`,
  run under NSH's `CallCx::with_owner`, so TSX and Rust screens nest either way; screens under
  JS owners of their own are isolated, views placed side by side in one component share their
  owner's scope), `view_model_ts.rs` (TS `ViewModelActivator`, `ActivationToken`, `DisposeBag`,
  `useViewModel`, `Command` with `IgnoreWhileRunning`, over `dxn_reactive`'s), `provider_ts.rs`
  (TS `ServiceProvider`, `ServiceToken`, `ServiceClass` and the registry of service classes and
  resolvers that sets fill; `view_model_class::<C>()` names a Rust view model by its TS class,
  resolved once and activated as that instance), `container_ts.rs` (TS services as thread-bound entries under runtime keys, made and
  resolved on the engine thread), `startup_ts.rs`, `contract_ts.rs` (the contracts' TS adapters,
  `ContractState`/`ContractEvents`, `registry.contract::<dyn T>()` and
  `contract_class::<dyn T, Class>()`).
- `configuration/`: `settings.rs` (`appsettings.json`), the TS options and configuration builder.
- `binding.rs`: dXn.Reactive and published values (`use_state_cell`, `use_event_stream`) as NSH
  signals, and `EngineRunner` (one per surface, `Send + Sync + Clone`: `run`/`post` a job on the
  engine thread from any thread).
- `services/`: only what service crates build on (the core's `services` re-exported, `json`,
  the TS listeners helper); `devtools/`; `ts.rs` (the public `ts` module and `register`);
  `testing.rs` (feature `testing`: the headless app harness service crates' tests use).

Contracts are callable from any thread: a TS implementation's adapter starts the call as an
engine task on the engine thread, else queues it on the surface's dispatch queue drained by the
engine; once the engine is gone calls end in `ContractError::EngineGone`. A Rust implementation is
called directly. State, events and constants are linked once when the adapter is made and then
read without calling JS. A contract may take a RayX TS class as its token, so TS holds one class
per service whichever implementation is registered.

The umbrella `rayx` re-exports `rayx::app` (core), `rayx::app_nsh` (host), `rayx::services::<name>`
per enabled feature (one per service crate; defaults `platform`, `suspension`, `assets`, `fonts`;
no feature implies another), `rayx::components`, `rayx::devtools_protocol`, `i18n`/`intl` and
`checked` (with `i18n`), a prelude with each enabled crate's builder extension, and
`#[rayx::native_module("spec")]`, whose expansion calls `rayx::__private::register` (the host's
classes, each enabled crate's set, then `registry.serve(rayx_components::set())`: RayX's
components, roles, theme document and `Previews<C>` are the served set `rayx/components`, not the
app's) before the function's body. Its line in the source is what NSH's CLI reads the set's
specifier from, so it stays an attribute. The headless harness (`src/testing.rs`) bundles with
`Registry::served_sets(..)`, so test TSX imports from `rayx/components`.

## Backlinks

Code and docs: `crates/rayx_app_nsh/src/lib.rs`, `crates/rayx_app_nsh/src/ts.rs`,
`crates/rayx/src/lib.rs`, `crates/rayx/Cargo.toml`, [docs/architecture.md](../../../../docs/architecture.md),
[docs/tsx.md](../../../../docs/tsx.md), [docs/startup.md](../../../../docs/startup.md),
[docs/view-models.md](../../../../docs/view-models.md); tests `crates/rayx_app_nsh/tests/nsh_*.rs`
(`nsh_tsx_*` for the TS side, `nsh_tsx_contracts_any_thread`, `nsh_engine_runner`,
`nsh_required_services`, `nsh_service_crate_hooks`, `nsh_app_extension_methods`, `source_layout`),
`crates/rayx/tests/` (`umbrella`, `features`, `native_module`, `native_module_attribute`,
`crate_layout`).

Memory: [core](../core/state.md), [runtime](../runtime/state.md),
[library set decision](../../decisions/components/library-set/2026-11-20-components-library-set-served-by-every-app.md) (current, 2026-11-20),
[dependencies](../dependencies/state.md) (the NSH pin),
[TSX contracts and builder extensions decision](../../decisions/app/tsx/2026-11-17-app-tsx-contracts-any-thread-builder-extensions.md) (current, 2026-11-17).
