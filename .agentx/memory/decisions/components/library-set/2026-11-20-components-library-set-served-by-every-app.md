---
agentx:
  kind: decision
  scope: decisions/components/library-set
  status: current
  created: 2026-11-20T12:00:00+01:00
  updated: 2026-11-20T12:00:00+01:00
  topics:
    - rayx_components
    - rayx/components
    - library-set
    - previews
    - assets
    - native_module
  supersedes: []
  superseded_by: []
---
# Decision: RayX's components are the library set `rayx/components`, served by every app

**Date:** 2026-11-20, decided with the owner. **Status:** current. It builds on the roles
decision ([role list and role map](../roles/2026-11-19-components-roles-role-list-and-map.md):
the library carries RayX's roles and default theme) and on NSH's library sets
(`Registry::serve`, the fork's `docs/dev/rust-modules.md` "A library's set"). A clean end state,
not a stopgap.

## Context

`#[rayx::native_module("app/native")]` registered RayX's components, roles and theme document
into each app's own UI set, so TSX imported `Button` from an app-specific specifier, and
`Previews<C>` came from the app's set too. Nothing written once (a component's preview file, the
docs, a design tool) could name RayX's components the same way in every app, and a library could
not ship its own previews. NSH now lets a library's crate make a set under its own fixed
specifier that apps serve (`Registry::serve`), refuses two sets registering one component, lets
TSX import a file a served set ships (`import x from "asset:/@pkg/path"`), and lists the files
every served set ships while the engine runs (`served_asset_files()`).

## Decision

- **One set, one name.** `rayx_components` makes its own UI set, `rayx/components`
  (`SPECIFIER`, `set()`: the theme document, RayX's roles, `Previews<C>` and `PreviewState<P>`,
  the components). It still depends on `nsh-core` and `nsh-base` alone; there is no
  `register(registry)` into another set any more.
- **Every app serves it.** `rayx::__private::register` (what `#[rayx::native_module]` expands to)
  calls `registry.serve(rayx_components::set())` and registers none of it in the app's set; a
  hand-written `#[native_module]` does the same. Nothing is listed in `nsh.json`; Rust users keep
  the prelude. RayX's theme document stays the app's theme unless the app's own set has one.
- **Every TSX file imports RayX's components and the previews' types from `rayx/components`**
  (apps, RayX's docs, the gallery, the `rayx new` templates' wording, the skills).
- **Libraries ship their previews beside their components.** Each component of RayX has
  `src/<snake_name>.preview.tsx` beside its source, literal data typed with `Previews<C>`,
  importing from `rayx/components`, with an `id` unique in the crate; a new component lands with
  it (`crates/rayx_components/tests/previews.rs`). Preview ids are unique within their package.
- **Libraries ship their files beside their components**, listed by their set
  (`registry.assets(assets!())`). TSX names a file it needs checked at build time
  (`import x from "asset:/@pkg/path"`); RayX's asset catalog takes every served set's files when it
  loads (`served_asset_files()` through the surface's engine runner), after the app's own packages,
  so an app finds a library's files without naming its package.

## Alternatives considered

- Keep registering the components into each app's set: rejected, an app-specific import breaks
  every file written once for all apps (previews, docs, a design tool's stage).
- Listing `rayx_components` in each app's `nsh.json` `"rust"`: rejected; NSH's library sets need
  no listing, and an app names RayX once, in Cargo.

## Trade-offs accepted

A breaking change for every app's TSX (`import { Button } from "rayx/components"`) and for
hand-written sets (`registry.serve(rayx::components::set())` in place of
`rayx_components::register`). An app registering a RayX component in its own set is refused when
described. Host test harnesses must give the bundler `Registry::served_sets(..)`, not the app's
set alone.

## Supersedes

None.
