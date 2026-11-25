---
agentx:
  kind: state
  scope: knowledge/dependencies
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-25T12:15:00+01:00
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
integration tag `nshx-*` in `[patch.crates-io]`, today `nshx-0.7.0-1` (commit `d49d11bf3`; NSH 0.7.0: upstream NSH up to `3f8ecd8b3` (sync dXnSix/nsh#299), with the library sets' work RayX built on a local checkout from 2026-11-20 (`Registry::serve`, a set's asset imports, a component two sets register refused) and a call's Rust code under its JS owner; upstream moved `Speech` out of NSH's runtime into the opt-in `nsh-extra` (#2947), so `nsh_core` has no `speech_available()` and `PlatformFeatures` lost its `speech` field; the `nsh-*` requirements are 0.7.0; `nsh-theme` is unchanged since `-6`. Before it, `nshx-0.6.0-7` (commit `6e14d3507`; `-7` takes upstream NSH up to `697857f6a` (sync dXnSix/nsh#289) and adds, for RayX Studio's phase 2, `nsh run`/`nsh dev --app <dir>` (an entry kept outside its app, #290), `import.meta.glob` (#291) and the dev hosts' design mode, `NSH_DESIGN_SOCKET` with `nsh/design` (#292); `nsh-theme` is unchanged; `-6` (`265966565`) adds DockArea edge docking, dXnSix/nsh#288: a dock's last panel can leave and the emptied dock stays in the layout closed and empty at its size, a tab dropped at an area edge makes or reopens the dock there, and `DockPanelOptions.placements` keeps a panel to some regions, which RayX Studio can use in place of its "Move to … Dock" menu and layout revert; `nsh-theme` is unchanged; `-5` (`2d6a0615c`) makes Rust's `set_color_scheme` (from a synchronous JS call, a task or a component) reach TS's `colorScheme()` and `Appearance::System` show the system's scheme again, dXnSix/nsh#286, with upstream sync #287 (embedder-only `Core::appearance()`, `nsh_js::Host::set_appearance` and the required `nsh_engine::browser::Host::set_appearance`, which RayX does not implement); `-4` (`aa5757480`) added Rust's theme writes, `set_modifier`, `set_color_scheme`, `update_tokens` and the token resets, queued with a synchronous JS call's turn, #285; it patches every NSH crate, `nsh-theme` too, the `nsh-*` requirements are 0.6.0, and RayX.Dtcg's `nsh-theme` stays on `nshx-0.6.0-5`, whose `nsh-theme` equals `-6`'s, `-7`'s and `nshx-0.7.0-1`'s). Earlier tags, up to `nshx-0.5.0-7` (commit `d47d7fd6d`:
`JsHandle`, `Constructor`, `#[interface]`, typed `#[ts("…")]` signatures and theme documents
since `nshx-0.5.0-4`; `-5` adds asking whether a kept JS value can be called now, #269; `-6` adds
class extensions, methods a crate adds to another crate's `#[class]`, #273, and upstream NSH up to
`afdeb025d`; `-7` reads JS properties and interface getters between JS turns and adds
`runtime_entered()` and `start_task()`, #274).
The CLI's built-in default nsh is the same tag (`crates/rayx_cli/src/project/pins.rs`, checked by
`tests/nsh_pin_rayx.rs`), and so is the gallery's `.cargo/config.toml`; the three move together,
and `rayx nshx use <tag>` moves them.

`rayx nshx use local` points the patch lines of `Cargo.toml` and the gallery's `.cargo/config.toml`
at the fork's checkout beside RayX (`../../dXn/nsh/nsh/crates/<crate>` from RayX's root, relative)
and moves the `nsh-*` requirements to the checkout's version. That is for a session in RayX's own
checkout only, and never committed: the path is relative to where the checkout sits, so a git
worktree (where agents work) does not reach the fork from it, CI cannot resolve it, and
`tests/nsh_pin_rayx.rs` fails while it is set. `main` stays on a tag; on 2026-11-19 it was on the
local checkout for a day and moved back to `nshx-0.5.0-7` for that reason.

## Backlinks

Code and docs: `Cargo.toml`, `AGENTS.md` ("Dependencies"), `crates/rayx_cli/tests/dependency_sources.rs`.

Decision: [one NSH tag across RayX and RayX.Dtcg](../../decisions/dependencies/nsh-pin/2026-11-19-dependencies-nsh-pin-one-tag.md) (2026-11-19, current).

Memory: [app](../app/state.md), [cli](../cli/state.md) (the `rayx nshx use` follow-up).
