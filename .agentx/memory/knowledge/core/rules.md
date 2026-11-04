---
agentx:
  kind: rules
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
# core: rules


- `rayx_core` never depends on `nsh-core` or any NSH crate, and must keep building with
  `cargo check -p rayx_core --target wasm32-unknown-unknown`.
- Each feature's tests go in their own group `crates/rayx_core/tests/<feature>.rs`.
