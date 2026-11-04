---
agentx:
  kind: state
  scope: knowledge/core/di
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
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

## Backlinks

Code and docs: [docs/dependency-injection.md](../../../../../docs/dependency-injection.md),
[docs/startup.md](../../../../../docs/startup.md); tests `di_provider`, `di_startup`,
`di_startup_timeout`, `di_app_builder` in `crates/rayx_core/tests/`.

Memory: [core](../state.md), [app](../../app/state.md).
