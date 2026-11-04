---
agentx:
  kind: state
  scope: knowledge/core/configuration
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - configuration
    - options
  supersedes: []
  superseded_by: []
---
# core/configuration: state

## Purpose And Usage

Context for changes to configuration sources and options binding.

## Current Technical Shape

`crates/rayx_core/src/configuration` and `src/options`. The bundled
`appsettings.json` is read on NSH by `rayx_app` (`src/settings.rs`).

## Backlinks

Code and docs: [docs/configuration.md](../../../../../docs/configuration.md); tests
`di_configuration`, `di_options` in `crates/rayx_core/tests/`.

Memory: [core](../state.md), [runtime](../../runtime/state.md) (application configuration).
