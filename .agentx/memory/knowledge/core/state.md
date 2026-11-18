---
agentx:
  kind: state
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
# core: state

## Purpose And Usage

Route work on `rayx_app`, the core every other RayX crate builds on, and on the macros.

## Current Technical Shape

- `rayx_app`: dependency injection (`src/di`), change listeners (`src/change.rs`), configuration
  sources (`src/configuration`), the options model (`src/options`), `RayXAppBuilder` and the
  `AppBuilder` traits service crates extend (`src/services/app`). No NSH dependency; builds for
  `wasm32-unknown-unknown`.
- Published values (`src/published.rs`): `StateCell<T>` (latest value as `Arc<T>`, `publish`
  notifies outside the lock on the publishing thread) and `EventStream<E>` (no value kept), what
  a contract's `#[state]` and `#[events]` members return.
- Each service's contract in `rayx_app::services::<name>` (`src/services/<name>/`): a
  `#[contract]` trait named after the service (`KeyValueStore`, `PlatformInfoService`,
  `I18nService`, …) with its data types, plus Rust helpers on `dyn Trait` and lookups as
  functions of the state types; `MessageFormatter` is the one Rust-only, synchronous trait (i18n).
  A member is a command (`async fn` or posted `fn`), a published value (`#[state]`, `#[events]`)
  or a constant (`#[constant]`).
- `rayx_app::testing` (`src/testing.rs`): an NSH-free fake per contract, and `add_host_fakes`
  for the four services the NSH host requires.
- `rayx_app_macros`: `#[derive(AppService)]` (found through `rayx`, else `rayx_app`, else
  `rayx_app_nsh`), `#[contract]` (`src/contract.rs`), `contract_nsh!`, `#[app_extension]`
  (`src/extension.rs`) and `#[native_module]` (`src/native_module.rs`); the last three name
  `rayx_app_nsh` or `::rayx::app_nsh`.

## Backlinks

Code and docs: `crates/rayx_app/src/lib.rs`, `crates/rayx_app/src/services/mod.rs`,
`crates/rayx_app/src/published.rs`, `crates/rayx_app/src/testing.rs`,
`crates/rayx_app_macros/src/lib.rs`, [docs/architecture.md](../../../../docs/architecture.md),
[docs/services.md](../../../../docs/services.md); tests `crates/rayx_app/tests/*.rs` (`di_*`,
`change_listeners`, `published_values`, `testing_fakes`, `*_contract`), `crates/rayx/tests/derive.rs`,
`crates/rayx/tests/macro_paths.rs`.

Memory: [di](di/state.md), [configuration](configuration/state.md), [runtime](../runtime/state.md),
[crate structure decision](../../decisions/runtime/service-crates/2026-11-16-runtime-service-crates-crate-structure.md) (current, 2026-11-16),
[taxonomy decision](../../decisions/governance/memory/2026-11-04-governance-memory-taxonomy.md) (current, 2026-11-04).
