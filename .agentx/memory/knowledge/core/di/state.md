---
agentx:
  kind: state
  scope: knowledge/core/di
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - dependency-injection
    - startup
    - validation
    - AppService
  supersedes: []
  superseded_by: []
---
# core/di: state

## Purpose And Usage

Context for changes to the service provider, scopes, validation and startup.

## Current Technical Shape

`crates/rayx_app/src/di` (provider, registry, keys, thread-bound values, lifecycle and startup,
validation) and `src/services/app` (`RayXAppBuilder`, the builder extension traits); the derive
macro in `crates/rayx_app_macros`. On NSH, `rayx_app_nsh` ties the lifetimes to a root provider
per surface and a scope per screen ([app](../../app/state.md)).

- Keys: a `ServiceKey` is a Rust type (a `dyn Trait` for a contract) or a `RuntimeKey`
  (`add_*_keyed`, `service_by_key`), sharing one set of caches, guards and scope rules.
- Thread-bound values (neither `Send` nor `Sync`, `add_*_thread_bound`): a `Send + Sync` slot in
  the provider, the value in a thread-local table of its thread, `ServiceResolutionError::WrongThread`
  elsewhere, never dropped off its thread. The host keeps TS services and TS options this way.
- Validation (`src/di/validation.rs`): `require::<dyn T>(hint)` (the app or host needs `T`) and
  `require_for::<dyn S, dyn T>(hint)` (the registration of `S` depends on `T`).
  `RayXAppBuilder::build`/`build_async` check them before any factory, option validation or
  startup service runs; every missing key, and every singleton requiring a scoped service, is one
  line of one `ServiceValidationError` (`AppBuildError::Services`).
- Startup: keyed by type or runtime key; `add_startup_resolved_async` makes one asynchronously
  when startup runs (bounded by the default timeout); `startup_service_type` defaults to the
  registration key. Ordering names traits: an implementation registered under its private type
  reports `StartupServiceType::of::<dyn Trait>()` and is aliased as `dyn Trait`, so dependents
  naming the trait wait for whichever implementation is registered. A dependency on a key that
  only a non-startup service is registered under (an app's own implementation, a fake) is met at
  once; one nothing is registered under fails startup (`StartupRegistry`, `src/di/lifecycle.rs`).

## Backlinks

Code and docs: [docs/dependency-injection.md](../../../../../docs/dependency-injection.md),
[docs/startup.md](../../../../../docs/startup.md); tests `di_provider`, `di_startup`,
`di_startup_timeout`, `di_startup_first`, `di_app_builder`, `di_runtime_keys`, `di_thread_bound`,
`di_startup_runtime`, `di_startup_registration_key`, `di_startup_plain_dependency`,
`di_validation`, `app_ready_hooks` in `crates/rayx_app/tests/`.

Memory: [core](../state.md), [app](../../app/state.md),
[crate structure decision draft](../../../decisions/runtime/service-crates/2026-11-16-runtime-service-crates-crate-structure.md) (current, 2026-11-16).
