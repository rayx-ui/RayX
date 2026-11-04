---
agentx:
  kind: state
  scope: knowledge/runtime
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx_runtime
    - services
  supersedes: []
  superseded_by: []
---
# runtime: state

## Purpose And Usage

Route work on the app services.

## Current Technical Shape

`crates/rayx_runtime/src/services`: storage, operations and history, navigation,
selection, input maps, platform and suspension (lifecycle) state, application configuration,
assets, fonts, i18n. What a service needs from the platform is a trait here that `rayx_app`
implements. Depends on `rayx_core` alone; builds for `wasm32-unknown-unknown`.

## Backlinks

Code and docs: [docs/services.md](../../../../docs/services.md) and one doc per service in
[docs/services/](../../../../docs/services/); tests one group per service in
`crates/rayx_runtime/tests/`.

Memory: [core](../core/state.md), [app](../app/state.md).
