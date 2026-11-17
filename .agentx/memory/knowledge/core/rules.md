---
agentx:
  kind: rules
  scope: knowledge/core
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-17T14:00:00+01:00
  topics:
    - rayx_app
    - rayx_app_macros
  supersedes: []
  superseded_by: []
---
# core: rules


- `rayx_app` (the core; `rayx_core` before the service-crates epic) never depends on `nsh-core` or any NSH crate, and must keep building with
  `cargo check -p rayx_app --target wasm32-unknown-unknown`.
- Each feature's tests go in their own group `crates/rayx_app/tests/<feature>.rs`.
