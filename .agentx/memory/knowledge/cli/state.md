---
agentx:
  kind: state
  scope: knowledge/cli
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-19T13:30:00+01:00
  topics:
    - rayx_cli
    - rayx-cli
    - nsh-pin
  supersedes: []
  superseded_by: []
---
# cli: state

## Purpose And Usage

Route work on the command-line tool.

## Current Technical Shape

Modules `cli`, `host`, `project` (pins), `setup` (doctor, WSL and Lima guests), `nsh`
(pin, store, install, run), `app`, `new`, `serve`, `fmt`, `update`. Moved here from
`rayx-ui/RayX.CLI` on 2026-11-02.

`rayx new`'s RayX templates (`src/new.rs`, the files in `src/new/`) write RayX's app builder API
(epic `ts-app-builder`, sprint `s4-consumers`): a `mixed` app's `src/lib.rs` is
`#[rayx::native_module("<specifier>")] fn native(registry: &mut Registry)` adding RayX's dev tools
(`DevTools`, `DevLayer`), and its `App.tsx` builds `new RayXAppBuilder().addStorage().addHistory()`, awaits
`start()` and resolves `KeyValueStore`; a `rust` app's root component calls
`RayXAppBuilder::new().add_storage().add_history().start()` and shows `app_state()`, its set
`#[rayx::native_module("<specifier>")]` registering `App`. The specifier
comes from nsh init's own `#[native_module("…")]` (`native_module_specifier`).

## Open follow-ups

- `rayx nshx use <tag>` leaves two copies of the old pin behind, so every repin needs two hand
  fixes (found in the epic `theme-roles`, 2026-11-19): the pin's short commit hash where tests
  cite it, and the percent-encoded form of the tag (`nshx%2D…`). The command should move both.

## Backlinks

Code and docs: [crates/rayx_cli/README.md](../../../../crates/rayx_cli/README.md),
[docs/cli/decisions.md](../../../../docs/cli/decisions.md),
[docs/cli/projects.md](../../../../docs/cli/projects.md),
[docs/cli/architecture/rayx-nsh-proposal.md](../../../../docs/cli/architecture/rayx-nsh-proposal.md);
tests `crates/rayx_cli/tests/<feature>.rs` (`new_app_builder`, `new_app`, `new_devtools` for the
templates), `#[ignore]`d `proof_*` groups.

Memory: [dependencies](../dependencies/state.md), [releasing](../releasing/state.md).
