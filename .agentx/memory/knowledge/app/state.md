---
agentx:
  kind: state
  scope: knowledge/app
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - rayx_app
    - nsh
    - tsx
    - rayx
  supersedes: []
  superseded_by: []
---
# app: state

## Purpose And Usage

Route work on the NSH-facing layer and the umbrella.

## Current Technical Shape

`rayx_app` holds a root provider per engine surface (`surface.rs`), a scope per screen
(`screen.rs`, `scope_class.rs`), view-model binding (`binding.rs`), navigation regions and the
location (`region.rs`, `location.rs`), the TSX classes (`service_classes.rs`, `i18n_class.rs`) and
the NSH side of the services (`settings.rs`, `storage.rs`, `platform.rs`). It is the only crate
that depends on `nsh-core`. `rayx` re-exports `rayx_core`, `rayx_runtime`, `rayx_app`,
`dxn_i18n` and `dxn_intl` with a prelude, and holds no code of its own.

## Backlinks

Code and docs: [docs/view-models.md](../../../../docs/view-models.md),
[docs/navigation.md](../../../../docs/navigation.md), [docs/tsx.md](../../../../docs/tsx.md);
tests `crates/rayx_app/tests/nsh_*.rs`, `crates/rayx/tests/umbrella.rs`.

Memory: [dependencies](../dependencies/state.md) (the NSH pin), [runtime](../runtime/state.md).
