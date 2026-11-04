---
agentx:
  kind: state
  scope: knowledge/core
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx_core
    - rayx_app_macros
  supersedes: []
  superseded_by: []
---
# core: state

## Purpose And Usage

Route work on `rayx_core`, the crate service crates depend on alone.

## Current Technical Shape

Dependency injection (`src/di`, `src/services`), change listeners (`src/change.rs`),
configuration sources (`src/configuration`) and the options model (`src/options`), assembled by
`RayXAppBuilder`. `rayx_app_macros` provides `#[derive(AppService)]`, re-exported by `rayx_core`,
`rayx_app` and `rayx`. No NSH dependency; builds for `wasm32-unknown-unknown`.

## Backlinks

Code and docs: `crates/rayx_core/src/lib.rs`, `crates/rayx_app_macros/src/lib.rs`,
[docs/architecture.md](../../../../docs/architecture.md); tests `crates/rayx_core/tests/*.rs`
(`di_*`, `change_listeners`), `crates/rayx/tests/derive.rs`.

Memory: [di](di/state.md), [configuration](configuration/state.md),
[taxonomy decision](../../decisions/governance/memory/2026-11-04-governance-memory-taxonomy.md) (current, 2026-11-04).
