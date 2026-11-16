---
agentx:
  kind: state
  scope: knowledge/core/di
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-16T23:00:00+01:00
  topics:
    - dependency-injection
    - startup
    - AppService
  supersedes: []
  superseded_by: []
---
# core/di: state

## Purpose And Usage

Context for changes to the service provider, scopes and startup.

## Current Technical Shape

`crates/rayx_core/src/di` (provider, scopes, startup) and `src/services`; the derive
macro in `crates/rayx_app_macros`. On NSH, `rayx_app` ties the lifetimes to a root provider per
surface and a scope per screen.

Services are keyed by a `ServiceKey`: a Rust type or a `RuntimeKey` (`add_*_keyed`,
`service_by_key`), sharing one set of caches, guards and scope rules. Values that are neither
`Send` nor `Sync` are thread-bound (`add_*_thread_bound`): a `Send + Sync` slot in the provider,
the value in a thread-local table of its thread, `WrongThread` elsewhere, never dropped off its
thread; a registration records its kind, so a resolve through the other API makes nothing. Startup
services are keyed by type or runtime key; `add_startup_resolved_async` makes one asynchronously
when startup runs (bounded by the default timeout under a timer), and `startup_service_type` is
optional, defaulting to the registration key (epic `ts-app-builder`, sprint `s1-core`). The TS
side builds on these: `rayx_app::ts` registers each TS service and TS options as a thread-bound
entry under a runtime key, and each TS startup service through `add_startup_resolved_async`
([app](../../app/state.md)).

## Backlinks

Code and docs: [docs/dependency-injection.md](../../../../../docs/dependency-injection.md),
[docs/startup.md](../../../../../docs/startup.md); tests `di_provider`, `di_startup`,
`di_startup_timeout`, `di_app_builder`, `di_runtime_keys`, `di_thread_bound`, `di_startup_runtime`,
`di_startup_registration_key` in `crates/rayx_core/tests/`.

Memory: [core](../state.md), [app](../../app/state.md).
