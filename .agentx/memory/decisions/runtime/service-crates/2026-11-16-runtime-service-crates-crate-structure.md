---
agentx:
  kind: decision
  scope: decisions/runtime/service-crates
  status: current
  created: 2026-11-18T03:10:00+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - crates
    - services
    - contracts
    - dependency-injection
    - features
  supersedes: []
  superseded_by: []
  promoted_from: service-crates
---
# Decision: RayX's crates: a core of contracts, an NSH host, a crate per service, an umbrella of features

**Date:** 2026-11-16 (names, required services, defaults) and 2026-11-17 (layout, contracts),
decided with the owner. **Status:** drafted from the design of the epic `service-crates`
(`.agentx/tasks/service-crates/design.json`: `d-crate-names`, `d-layout`, `d-abstractions`,
`d-i18n-split`, `d-validation`, `d-plain-startup-dependency`, `d-service-crates`, `d-umbrella`,
`d-template`); source on `main` already follows it. **Promotion** happens when that epic closes,
as AgentX promotes an epic's decisions; this note then goes. It supersedes no repository
decision; it changes which crates the [taxonomy decision](../../governance/memory/2026-11-04-governance-memory-taxonomy.md)'s
domains cover (`core`: `rayx_app`, `runtime`: `rayx_services_*`, `app`: `rayx_app_nsh` and `rayx`).

## Context

RayX's services lived in one crate, `rayx_runtime`, as concrete types consumers held by value;
only platform seams were traits. Every app compiled every service, `RayXAppBuilder::new()`
pre-registered NSH's, a missing dependency panicked in a factory, and startup ordering named
concrete types a replacement could never satisfy. The owner wants true abstractions anyone can
implement in Rust or TS, apps that compile and register only what they use, and a clear error
naming what to register.

## Decision

- **Crates.** `rayx_app` is the NSH-free core (`rayx_core` renamed): DI, configuration, options,
  the builder, `StateCell`/`EventStream`, every service's `#[contract]` trait and data types in
  `rayx_app::services::<name>`, an NSH-free fake of each in `rayx_app::testing`. `rayx_app_nsh`
  is the NSH host (the old `rayx_app`), by topic folder (`app/`, `di/`, `configuration/`,
  `binding.rs`, `devtools/`, `services/` holding only what service crates build on, `ts.rs`,
  `testing`). Twelve `rayx_services_<name>` crates hold RayX's implementations, each with its
  NSH and TS side, options, Rust builder extension, TS builder method and `register`.
  `rayx_runtime` is gone. One version, one tag, one changelog (`rayx_app` owns them in
  release-plz).
- **Dependency rules.** The core depends on no NSH crate and no RayX crate but
  `rayx_app_macros`; the host builds on the core and depends on no service crate; a service crate
  depends on `rayx_app`, `rayx_app_nsh`, `rayx_app_macros` and `nsh-core` as it needs, never on
  another service crate. A service needing another resolves it as `dyn Trait`, so mocks and
  replacements work. `crates/rayx/tests/crate_layout.rs` checks the rules.
- **Nothing pre-registered; requirements validated.** `RayXAppBuilder::new()` is empty and no
  registration registers another service. `require`/`require_for` declare needs; the build checks
  them before any factory and reports every missing service and lifetime mismatch in one
  `ServiceValidationError`, each with a hint naming the call that adds it. The host requires the
  platform, suspension, asset and font services at start. Startup ordering names traits, and a
  dependency met by a non-startup implementation does not wait.
- **I18n is two traits.** The `I18nService` contract (state and async commands) and the Rust-only
  synchronous `MessageFormatter`, so `t()` and ICU formatting stay synchronous for any
  implementation.
- **The umbrella's features.** `rayx` has one optional feature per service crate, named after the
  service, implying no other; the defaults are the four the host requires. `rayx::app`,
  `rayx::app_nsh` and `rayx::services::<name>` re-export the crates, and `#[rayx::native_module]`
  registers the enabled crates' sets. `rayx new` turns on `storage`, `operations` and `history`
  and registers the four required services explicitly.

## Alternatives considered

- **Keep `rayx_runtime` with features per service**: consumers would still hold concrete types,
  and the services would stay unreplaceable.
- **Pre-register the required services**: users could not replace them cleanly; a missing one is
  instead an error with a hint, as ASP.NET Core does.
- **Features implying their dependencies** (history implying operations): hides what is
  registered; a missing dependency is a startup error with its hint like any other.
- **Name the core `rayx_core` and the host `rayx_app`**: the owner chose `rayx_app` for the core
  apps are built on and `_nsh` for the host implementing it on NSH.

## Trade-offs accepted

- Breaking renames (`rayx_core` -> `rayx_app`, the old `rayx_app` -> `rayx_app_nsh`,
  `rayx::core` -> `rayx::app`, `rayx::app` -> `rayx::app_nsh`) with `Migration: apps:` footers.
- Apps without the default features must register the four required services themselves.
- Twelve service crates in the workspace and the release group, where one crate was.

## Supersedes

None.
