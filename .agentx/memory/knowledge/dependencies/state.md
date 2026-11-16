---
agentx:
  kind: state
  scope: knowledge/dependencies
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-17T12:00:00+01:00
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
release tag, and the workspace's own crates by path (`rayx` among them, for the gallery's
crate); NSH crates come from crates.io names patched to the fork `github.com/dXnSix/nsh` at an
integration tag `nshx-*` in `[patch.crates-io]`, today `nshx-0.5.0-5` (commit `ce14525bc`:
`JsHandle`, `Constructor`, `#[interface]`, typed `#[ts("…")]` signatures and theme documents
since `nshx-0.5.0-4`; `-5` adds asking whether a kept JS value can be called now, #269, and
upstream NSH up to `a4f08d746`).
The CLI's built-in default nsh is the same tag (`crates/rayx_cli/src/project/pins.rs`, checked by
`tests/nsh_pin_rayx.rs`), and so is the gallery's `.cargo/config.toml`; the three move together.

## Backlinks

Code and docs: `Cargo.toml`, `AGENTS.md` ("Dependencies"), `crates/rayx_cli/tests/dependency_sources.rs`.

Memory: [app](../app/state.md), [cli](../cli/state.md).
