---
agentx:
  kind: state
  scope: knowledge/releasing
  status: current
  created: 2026-11-04T19:30:18+01:00
  updated: 2026-11-04T19:30:18+01:00
  topics:
    - release-plz
    - cargo-dist
    - ci
  supersedes: []
  superseded_by: []
---
# releasing: state

## Purpose And Usage

Context for release, CI and version work.

## Current Technical Shape

One version and one tag `vX.Y.Z` for all crates; release-plz (git-only) opens the
release pull request and pushes the tag on merge; cargo-dist builds `rayx` for six targets and
creates the GitHub Release. CI (`.github/workflows/ci.yml`) runs formatting, Clippy, tests and the
wasm check, and `rayx-cli` on Linux, macOS and Windows.

## Backlinks

Code and docs: [docs/releasing.md](../../../../docs/releasing.md), `release-plz.toml`,
`dist-workspace.toml`, `.github/workflows/`, `crates/rayx_cli/tests/ci_workflow.rs`.

Memory: [cli](../cli/state.md).
