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
integration tag `nshx-*` in `[patch.crates-io]`, today `nshx-0.6.0-3` (commit `6ec371784`; it patches every NSH crate, `nsh-theme` too, the `nsh-*` requirements are 0.6.0, and RayX.Dtcg's `nsh-theme` uses the same tag). Earlier tags, up to `nshx-0.5.0-7` (commit `d47d7fd6d`:
`JsHandle`, `Constructor`, `#[interface]`, typed `#[ts("…")]` signatures and theme documents
since `nshx-0.5.0-4`; `-5` adds asking whether a kept JS value can be called now, #269; `-6` adds
class extensions, methods a crate adds to another crate's `#[class]`, #273, and upstream NSH up to
`afdeb025d`; `-7` reads JS properties and interface getters between JS turns and adds
`runtime_entered()` and `start_task()`, #274).
The CLI's built-in default nsh is the same tag (`crates/rayx_cli/src/project/pins.rs`, checked by
`tests/nsh_pin_rayx.rs`), and so is the gallery's `.cargo/config.toml`; the three move together.

## Backlinks

Code and docs: `Cargo.toml`, `AGENTS.md` ("Dependencies"), `crates/rayx_cli/tests/dependency_sources.rs`.

Memory: [app](../app/state.md), [cli](../cli/state.md).
