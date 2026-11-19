---
agentx:
  kind: decision
  scope: decisions/dependencies/nsh-pin
  status: current
  created: 2026-11-19T13:30:00+01:00
  updated: 2026-11-19T13:30:00+01:00
  topics:
    - nsh-fork
    - git-tags
    - nsh-theme
    - rayx-dtcg
  supersedes: []
  superseded_by: []
  promoted_from: theme-roles
---
# Decision: one NSH tag across RayX and RayX.Dtcg

**Date:** 2026-11-17, with the tag moves of 2026-11-18 and 2026-11-19 (owner's calls).
**Status:** current; promoted from the epic `theme-roles` (`.agentx/tasks/theme-roles/design.json`:
`D-PIN`, `D-PIN-RELEASED`, `D-PIN-FIRST-FRAME`, `D-COLOR-SCHEME-JS`), closed 2026-11-19.

## Context

RayX's default theme is written by RayX.Dtcg through the fork's `nsh-theme` crate and read by
RayX through `nsh-core`. Two NSH versions would let the exporter write a format RayX reads
differently.

## Decision

- RayX and RayX.Dtcg name **the same `nshx-*` tag** of `github.com/dXnSix/nsh` (today
  `nshx-0.6.0-5`). A tag move changes both repositories together.
- **RayX.Dtcg** depends on `nsh-theme` by that git tag in `crates/rayx_dtcg/Cargo.toml` (as
  `dxn_dtcg` is, so a move releases the crate); `nsh-theme` is the one NSH crate its CI guard allows.
- **RayX** patches every NSH crate, `nsh-theme` included, in `[patch.crates-io]`, and its `nsh-*`
  requirements (and the CLI's `DEFAULTS.nsh.version`) match the tag's version: a patch whose
  version misses the requirement goes unused.
- The pin's other copies move with it (the CLI's default pin, the gallery's `.cargo/config.toml`,
  the files `rayx new` writes, the citations in tests and docs, the dependencies memory).
- Git tags only; a tag is never moved; nothing builds NSH from a path or `.references`.

## Alternatives considered

- RayX.Dtcg writing the format itself without NSH: a second implementation of NSH's rules.
- Independent pins per repository: the export could drift from what RayX reads.

## Trade-offs accepted

Every fork fix RayX needs is a new tag for both repositories (the epic went `-3`, `-4`, `-5`:
bindable `outline`, Rust theme writes inside a synchronous JS call, JS's `colorScheme()`
following Rust's `set_color_scheme`).

## Supersedes

None. It extends the [dependencies rules](../../../knowledge/dependencies/rules.md) (git tags
only; the patch tag and the CLI's pin move together) to RayX.Dtcg.
