---
agentx:
  kind: state
  scope: knowledge/runtime
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-20T12:00:00+01:00
  topics:
    - rayx_services
    - services
  supersedes: []
  superseded_by: []
---
# runtime: state

## Purpose And Usage

Route work on RayX's implementations of the services. A service is two parts: its contract in
the core ([core](../core/state.md), `rayx_app::services::<name>`) and RayX's implementation here,
which an app may replace with its own in Rust or TS.

## Current Technical Shape

Thirteen crates `crates/rayx_services_<name>`: `platform`, `suspension`, `assets`, `fonts` (the
four the host requires, the umbrella's default features), `storage`, `navigation`, `i18n`,
`operations`, `history`, `selection`, `input_map`, `application_configuration`, `theme` (since
epic theme-roles: `ThemeService` on NSH, the system's light/dark and contexts on request;
`rayx new` enables it). Each holds:

- RayX's implementation (`service.rs` and its helpers) and its options;
- the NSH side (`nsh_*.rs`: NSH storage, asset reader, font registrar, location, …) and the TS
  side (`*_ts.rs`: the service's TS class named after the trait and its options class), with
  `contract_nsh!` naming the contract's TS interface (`KeyValueStoreContract`);
- `builder.rs`: the Rust extension `<Name>AppBuilderExt::add_<name>` and the TS builder method,
  an NSH class extension of the host's TS `RayXAppBuilder` (`<Name>Builder`,
  `#[class_extension(RayXAppBuilder)]`), acting on it through `RayXAppBuilder::extend_with` and
  `ts::record`;
- `register(registry)`: its TS classes, resolvers, contract and builder method, called by
  `#[rayx::native_module]` when the umbrella's feature is on.

A service that needs another resolves it by trait and declares it with `require_for` (fonts on
`dyn AssetService`; i18n on assets and platform; history on operations, and storage when
persistent; selection on navigation; input map on storage; application configuration on
platform). i18n has two traits: the `I18nService` contract and the Rust-only, synchronous
`MessageFormatter` (`add_message_formatter()` over any `dyn I18nService`), with `tr!` here.
Platform and suspension keep themselves current from their initialization; suspension wires NSH's
quitting. RayX's asset catalog takes, after the options' packages and the files the app added,
every file a UI set the app serves ships (`AssetReader::served_files`: on NSH
`served_asset_files()` read on the engine thread through the `EngineRunner`), so a library's files
(RayX's components') need no package named (`assets_served`, `nsh_served_assets`). `HistoryStore` stays public in `rayx_services_history` as its extension point; other
platform seams are private to their crate. Calls to NSH from other threads go through the host's
`EngineRunner`.

## Backlinks

Code and docs: `crates/rayx_services_*/src/lib.rs`, `crates/rayx_services_*/src/builder.rs`,
[docs/services.md](../../../../docs/services.md) and one doc per service in
[docs/services/](../../../../docs/services/); tests one group per behavior in
`crates/rayx_services_<name>/tests/`, cross-crate ones in `crates/rayx/tests/`
(`service_replacement`, `ts_service_contracts`, `history_over_operations`, `crate_layout`).

Memory: [core](../core/state.md), [app](../app/state.md),
[crate structure decision](../../decisions/runtime/service-crates/2026-11-16-runtime-service-crates-crate-structure.md) (current, 2026-11-16).
