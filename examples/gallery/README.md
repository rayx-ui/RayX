# RayX Gallery

RayX's components as the Figma file (**RayX - Design System**) shows them, in code: each component's
page has a playground of its Figma properties, its variants and sizes as the component set lays
them out, and the usage examples of its Figma page. The toolbar switches the theme's modes as
Figma's do: the appearance (Light, Dark), the accent (`Theme.Colors`: Cyan, Sky, Blue) and the
density (`Platform.Density`: Desktop, Touch).

It is a mixed NSH app: the screens are TSX, so they reload as you save; the components are Rust
(`rayx_components`), imported from the app's crate.

| Path | What it is |
| --- | --- |
| `nsh.json` | The app: `"rust": "native"` and no `"root"`, so `App.tsx` is the entry |
| `App.tsx` | The entry: mounts the gallery |
| `src/gallery.tsx` | The shell: the sidebar of components, the toolbar of modes, the page |
| `src/pages/<component>.tsx` | One page per component: `button.tsx` |
| `src/ui.tsx` | The gallery's own layout (pages, sections, captioned examples, choices) and RayX's text styles as classes |
| `.cargo/config.toml` | The fork's NSH for nsh's own build of the app (`.nsh/rust`, a workspace of its own, where RayX's `[patch.crates-io]` does not reach): the file `rayx new` writes, at RayX's pin. Move it with the root `Cargo.toml`'s patch |
| `native/` | The app's crate `rayx-gallery`: `#[native_module("rayx-gallery/native")]` registers RayX's library (`rayx_components::register`) and `Glyph`, the icons the examples use |
| `App.test.tsx` | `nsh test`: the page, the playground and the modes, headless |
| `native/tests/gallery_tsx.rs` | The same screens bundled and run by `cargo test`, without the nsh CLI |

## Run it

From `RayX/`, with the `rayx` this repository builds (it runs the nsh the project pins, `nshx-0.5.0-1`,
and installs it the first time):

```sh
cargo run -p rayx-cli -- app run -C examples/gallery                 # this desktop; TSX edits reload
cargo run -p rayx-cli -- app run -C examples/gallery --target web    # a browser (after `rayx setup --web`)
cargo run -p rayx-cli -- app check -C examples/gallery               # cargo check, then tsc against the crate's types
cargo run -p rayx-cli -- app test -C examples/gallery                # App.test.tsx, headless
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
   `native/tests/gallery_tsx.rs`.

## Notes

- The gallery's choices are Buttons, the current one `primary`, until RayX has SegmentedButtons
  and ToggleButton in code; its count badge is drawn from tokens until RayX has its Badge.
- `Glyph` (Lucide icons, ISC License) stands in for RayX's Icon (Figma `primitives/Icon`) until it is
  in code. It strokes in `currentColor`, so in a Button's icon slot it takes the label's color.
- Text styles are classes of their four parts (`ui.tsx` `type`); line height and letter spacing go
  through `var()`: `leading-(--spacing-…-line-height)`, `tracking-(--spacing-…-letter-spacing)`.
