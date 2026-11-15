---
agentx:
  kind: state
  scope: knowledge/components
  status: current
  created: 2026-11-15T03:00:00+01:00
  updated: 2026-11-15T03:00:00+01:00
  topics:
    - rayx_components
    - theme
    - button
    - gallery
  supersedes: []
  superseded_by: []
---
# components: state

## Purpose And Usage

Route work on RayX's components, its theme type and the gallery.

## Current Technical Shape

- `src/theme/`: `RayXTheme`, a hand-written `AppTheme` around the generated types of `tokens.rs`
  (`ThemeTokens`, `PlatformTokens` and their nested structs, the `Accent`, `Density`, `Os` and
  `RoleSet` enums, every mode's values); `roles.rs` maps RayX tokens onto nsh-base's roles;
  `rayx()`, `Modes`, `ComponentSize`, `bind_text_style`.
- `src/macros.rs`: `color!` (hex checked at compile time), `rayx_theme!`, `rayx_patch!`.
- `src/button.rs`: `Button` on nsh-base's `Button`, its look one `Look` memo of tokens.
- `src/themes.rs`: `Themes`, a module for TSX (`Themes.set(accent, density, os)`).
- `register(registry)` adds the theme type, the components and `Themes` to an app's UI set.
- `examples/gallery`: a mixed NSH app (`nsh.json` `"rust": "native"`), its crate `rayx-gallery`
  (a workspace member, `release = false`) registering the library and `Glyph`.

## Backlinks

Code and docs: [docs/components.md](../../../../docs/components.md),
[examples/gallery/README.md](../../../../examples/gallery/README.md); tests
`crates/rayx_components/tests/{theme_tokens,theme_macros,theme_view,theme_tsx,button_view,button_tsx}.rs`,
`examples/gallery/native/tests/gallery_tsx.rs`.

Memory: [app](../app/state.md), [dependencies](../dependencies/state.md) (the NSH pin).
