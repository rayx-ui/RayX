---
agentx:
  kind: rules
  scope: knowledge/core
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-18T03:10:00+01:00
  topics:
    - rayx_app
    - rayx_app_macros
    - contracts
  supersedes: []
  superseded_by: []
---
# core: rules

- `rayx_app` (the core; `rayx_core` before the service-crates epic) never depends on `nsh-core`,
  any NSH crate or any RayX crate but `rayx_app_macros`, and must keep building with
  `cargo check -p rayx_app --target wasm32-unknown-unknown` (`crates/rayx/tests/crate_layout.rs`
  checks the dependencies).
- A service's contract (its `#[contract]` trait) and every data type it uses live in
  `rayx_app::services::<name>` and cross to TS (serde); generic or closure-taking calls are Rust
  helpers on `dyn Trait`. Its fake goes in `rayx_app::testing`. Options belong to the
  implementation's crate, not the core.
- Every member of a contract is a command, a published value (`#[state]`, `#[events]`) or a
  constant; nothing in the trait says which implementation is behind it.
- Nothing is pre-registered: `RayXAppBuilder::new()` is empty and no registration registers
  another service. A needed service is declared with `require`/`require_for` and fails the build
  with its hint.
- Each feature's tests go in their own group `crates/rayx_app/tests/<feature>.rs`.
