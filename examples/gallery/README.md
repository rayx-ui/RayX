# RayX Gallery

RayX's components as the Figma file (**RayX - Design System**) shows them, in code: each component's
page has a playground of its Figma properties, its variants and sizes as the component set lays
them out, and the usage examples of its Figma page. The toolbar switches the theme:

- **Theme**: the theme document, RayX's default theme or the sample theme of `themes/`, through
  NSH's `setTheme` (the sample is a theme with tokens of its own that maps the roles the gallery
  reads, to show that the components and the pages follow any theme);
- **Color mode**: System (the system's light or dark), Light or Dark, through RayX's theme service;
- **one picker per other modifier** of the active document, with its contexts, through the theme
  service's `setContext`: RayX's `accent` (`Theme.Colors`: Cyan, Sky, Blue), `density`
  (`Platform.Density`: Desktop, Touch) and `os`; the sample's `brand`.

The pickers come from the theme service's snapshot, so they always list the active document's
modifiers. Every class of the screens names a role (`bg-surface-panel1`, `h-default-lvl6-height`,
`rounded-default`, `text-default-title1-bold`), never a token of a theme.

It is a mixed NSH app: the screens are TSX, so they reload as you save; the components are Rust
(`rayx_components`), imported from the app's crate.

| Path | What it is |
| --- | --- |
| `nsh.json` | The app: `"rust": "native"` and no `"root"`, so `App.tsx` is the entry |
| `App.tsx` | The entry: starts RayX with the four services the host requires (`addPlatform`, `addSuspension`, `addAssets`, `addFonts`) and the theme service (`addTheme`), keeps the theme service's snapshot for the toolbar, and mounts the gallery, which says in its toolbar if the start failed |
| `src/gallery.tsx` | The shell: the sidebar of components, the toolbar of the theme (document, color mode, modifiers), the page |
| `src/pages/<component>.tsx` | One page per component: `button.tsx` |
| `src/ui.tsx` | The gallery's own layout (pages, sections, captioned examples, choices) and RayX's text styles as classes of its typography roles |
| `themes/sample.json` | The sample theme: a palette, semantic tokens of its own that reference it, the modifiers `colorMode` and `brand`, and `roles` mapping every role the gallery reads onto its semantic tokens (never onto the palette) |
| `.cargo/config.toml` | The fork's NSH for nsh's own build of the app (`.nsh/rust`, a workspace of its own, where RayX's `[patch.crates-io]` does not reach): the file `rayx new` writes, at RayX's pin. Move it with the root `Cargo.toml`'s patch |
| `native/` | The app's crate `rayx-gallery`: its UI set is `#[rayx::native_module("rayx-gallery/native")]`, RayX's (every RayX TS class, the services of the `rayx` features its `Cargo.toml` names, the theme service's among them, and the component library with its roles and default theme) with `Glyph`, the icons the examples use, added in its function's body |
| `App.test.tsx` | `nsh test`: RayX's start, the page, the playground and the theme's switches, headless |
| `native/tests/gallery_tsx.rs` | The same screens bundled and run by `cargo test`, without the nsh CLI |
| `native/tests/gallery_theme.rs` | `cargo test`: every theme-reading class names a role, the sample theme maps the gallery's roles onto its semantic tokens, and the toolbar's switches draw what the active document resolves |

## Run it

From `RayX/`, with the `rayx` this repository builds (it runs the nsh the project pins, `nshx-0.6.0-4`,
and installs it the first time):

```sh
cargo run -p rayx-cli -- app -C examples/gallery run                 # this desktop; TSX edits reload
cargo run -p rayx-cli -- app -C examples/gallery run --target web    # a browser (after `rayx setup --web`)
cargo run -p rayx-cli -- app -C examples/gallery check               # cargo check, then tsc against the crate's types
cargo run -p rayx-cli -- app -C examples/gallery test                # App.test.tsx, headless
cargo test -p rayx-gallery                                           # the crate and its headless TSX tests
```

A Rust edit (in `native/` or `crates/rayx_components/`) rebuilds the app's hosts and restarts it; a
TSX edit reloads in place.

## Adding a component's page

1. Build the component in `crates/rayx_components` (the `rayx-components` skill in RayX.Root).
2. Add `src/pages/<component>.tsx`: a `Page` with a playground of every Figma property (the same
   names), the variants matrix, the sizes, and the Figma page's examples, each an `Example` with
   its caption. Components and slot content at `size="default"`, except in an example about sizes.
3. Add the page to `pages` in `src/gallery.tsx`, and its checks to `App.test.tsx` and
   `native/tests/gallery_tsx.rs`. Its classes name roles only (`gallery_theme` checks it); map any
   role it reads that the sample theme lacks in `themes/sample.json`.

## Notes

- The gallery's choices are Buttons, the current one `primary`, until RayX has SegmentedButtons
  and ToggleButton in code; its count badge is drawn from roles until RayX has its Badge.
- `Glyph` (Lucide icons, ISC License) stands in for RayX's Icon (Figma `primitives/Icon`) until it is
  in code. It strokes in `currentColor`, so in a Button's icon slot it takes the label's color.
- Classes name RayX's roles as NSH reads them: a color role by its name (`bg-surface-app`,
  `text-intent-danger-text`), a namespaced role without its namespace's prefix (`px-inset4` reads
  `spacing-inset4`, `rounded-default` `radius-default`), a text style by its typography role
  (`text-default-regular` sets the family, size, weight, line height and letter spacing).
