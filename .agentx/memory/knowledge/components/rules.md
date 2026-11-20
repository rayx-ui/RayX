---
agentx:
  kind: rules
  scope: knowledge/components
  status: current
  created: 2026-11-15T03:00:00+01:00
  updated: 2026-11-20T12:00:00+01:00
  topics:
    - rayx_components
    - theme
    - button
    - gallery
    - previews
  supersedes: []
  superseded_by: []
---
# components: rules

- `theme.json` is generated (`theme_export.py` in the `rayx-components` skill): never edit it;
  regenerate after a token export or a change to `roles.json` and keep `check` passing. Code reads
  roles only, through `theme::value`, and RayX sets no theme modifier itself.
- Component props are the Figma properties with their names and defaults (`size` defaults to
  `default`); runtime states are styles, not props.
- A component's look comes from roles only, through one memo; `class` and `style`
  place it.
- A new component lands with its feature tests (`<name>_view`, `<name>_tsx`), its gallery page,
  its `rayx-components` reference and its preview file `src/<snake_name>.preview.tsx` beside its
  source: literal data typed with `Previews<C>`, component and type imported from
  `rayx/components`, an `id` unique in the crate (`tests/previews.rs`).
- Components, roles, the theme document and the previews' types are registered in
  `rayx_components::set()` (`rayx/components`) only; an app's set serves it, never registers any
  of it. TSX imports RayX's components from `rayx/components`, never from an app's specifier.
- A file the library ships goes in its `assets/`, listed by `set()` (`registry.assets(assets!())`).
