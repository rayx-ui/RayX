---
agentx:
  kind: state
  scope: knowledge/dependencies
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - git-tags
    - nsh-fork
    - dxn
  supersedes: []
  superseded_by: []
---
# dependencies: state

## Purpose And Usage

Context for any change to `Cargo.toml` dependencies or the NSH pin.

## Current Technical Shape

`[workspace.dependencies]` in `Cargo.toml` takes dXn.Reactive, dXn.I18n and dXn.Intl by
release tag; NSH crates come from crates.io names patched to the fork `github.com/dXnSix/nsh`
at an integration tag `nshx-*` in `[patch.crates-io]`. The CLI's built-in default nsh is the same
tag (`crates/rayx_cli/src/project/pins.rs`, checked by `tests/nsh_pin_rayx.rs`).

## Backlinks

Code and docs: `Cargo.toml`, `AGENTS.md` ("Dependencies"), `crates/rayx_cli/tests/dependency_sources.rs`.

Memory: [app](../app/state.md), [cli](../cli/state.md).
