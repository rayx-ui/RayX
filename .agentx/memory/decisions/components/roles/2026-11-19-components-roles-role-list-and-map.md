---
agentx:
  kind: decision
  scope: decisions/components/roles
  status: current
  created: 2026-11-19T13:30:00+01:00
  updated: 2026-11-19T13:30:00+01:00
  topics:
    - rayx_components
    - roles
    - roles.json
    - role-map
    - theme
  supersedes: []
  superseded_by: []
  promoted_from: theme-roles
---
# Decision: RayX's roles are an API list in `roles.json`; the role map is design data

**Date:** 2026-11-17, decided with the owner. **Status:** current; promoted from the epic
`theme-roles` (`.agentx/tasks/theme-roles/design.json`: `D-ROLE-LIST`, `D-ROLE-MAP`,
`D-ROLE-NAMES`), closed 2026-11-19. It implements RayX.Root's
`decisions/workspace/2026-11-19-theme-roles-only.md` (components read roles only).

## Context

Once components read roles instead of tokens, RayX needs one place that says which roles exist and
one that says which design token plays each, kept so that a design rename never touches code.

## Decision

- **The role list,** `crates/rayx_components/roles.json`, in NSH's role-declaration shape, is the
  one source of RayX's roles and is maintained by hand as API. `build.rs` generates
  `theme::roles` from it (a typed constant per role, `all()`), the typed groups (`IntentRoles`,
  `LevelRoles`, `SizeRoles`, `SizeTextRoles`) are hand-written over the constants, and `register`
  registers `roles::all()`, so a theme lacking a required role is refused. `build.rs`, not a proc
  macro, keeps the crate on `nsh-core` and `nsh-base` alone.
- **The vocabulary** is the design system's semantic layer, not per-component roles:
  `surface-*`, `intent-*`, `interaction-*`, `spacing-<size>-lvl<n>-*`, `radius-*`,
  `border-width-*`, `font-*`, `text-*`, `shadow-*`, `blur-*`, each segment in kebab case. The
  prefixes are NSH's class namespaces, so TSX classes name the roles directly
  (`h-default-lvl5-height`, `bg-intent-primary-bg`).
- **The role map,** RayX.Design's `tokens/dtcg/rayx.role-map.json` (`{ "<role>": "<DTCG path>" }`,
  RayX's roles plus the NSH core and nsh-base roles it maps), says which token plays each role. It
  is design data: a token rename changes its line, not RayX's code.
- The exporter takes both as arguments (`--role-map` writes `roles`, `--roles` checks them); the
  full check against RayX's, nsh-base's and the core's declarations runs on RayX's side.

## Alternatives considered

- Roles as Rust consts or enums: the owner wanted them "stored in dictionary", with typed handles
  generated.
- Per-component roles: rejected for the shared semantic layer.

## Trade-offs accepted

`roles.json` is a breaking-change surface: renaming a role is a `feat(components)!:` change.

## Supersedes

None.
