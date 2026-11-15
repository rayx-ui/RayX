# Components and theme

`rayx_components` (`rayx::components` in the umbrella) is the RayX design system in code: the Figma file's tokens as one
NSH theme type, and its components, each the Figma component of the same name on the NSH base component it mirrors.
Components are written in Rust and exported to TSX with typed props. The full guide for each component (props,
examples, DO and DON'T) is the `rayx-components` skill in RayX.Root (`.claude/skills/rayx-components/`).

| Component | Figma | Base |
| --- | --- | --- |
| `Button` | `buttons/Button` | nsh-base `Button` |

## Use it

Register the library in the app's UI set. It adds the components, the theme type (RayX's tokens become the app's theme:
its classes, `token()` and the roles NSH's own parts draw with) and the `Themes` module:

```rust
use nsh_core::prelude::*;

#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    rayx::components::register(registry);
}
```

TSX imports them by the set's specifier; props are Figma's properties in camel case:

```tsx
import { Button, Themes } from "my-app/native";

<Button text="Save changes" variant="primary" onPress={save} />
<Button text="Inbox" iconLeft={<Icon name="inbox" />} showSlotRight slotRight={<Badge count={3} />} />
```

Rust views use the constructors; props are Figma's properties in snake case:

```rust
use rayx::components::prelude::*;

button().text("Save changes").variant(ButtonVariant::Primary).on_press(move || save())
```

`class` and `style` on a component place it (a margin, a width); they never restyle it. A look the library lacks is a new
variant in the library and in Figma.

## The theme

`RayXTheme` has the tokens' Figma WEB code syntax as its fields: `theme/colors/roles/primary/bgHover` is
`rayx().theme.colors.roles.primary.bg_hover`, `platform/density/size/default/lvl5/height` is
`rayx().platform.density.size.default.lvl5.height`. Read `rayx()` inside bindings so they follow the theme:

```rust
view()
    .bg(move || rayx().theme.colors.surfaces.panel1)
    .p(move || px(rayx().platform.density.insets.inset5))
    .child(bind_text_style(text("Settings"), |t| t.platform.density.typography.h3.clone()))
```

In TSX the tokens are classes: the token's path after its collection in kebab case behind its kind's namespace:
`bg-surfaces-app`, `text-roles-primary-text`, `gap-gap4`, `p-insets-inset5`, `h-size-default-lvl5-height`,
`rounded-borders-radius4`, `shadow-default`. Line heights and letter spacing are points, through `var()`:
`leading-(--spacing-typography-body2-line-height)`, `tracking-(--spacing-typography-body2-letter-spacing)`. So are the
shadows and blurs named as Tailwind's sizes (`sm`, `md`, `lg`, `xl`), whose classes stay Tailwind's:
`shadow-(--shadow-md)`, `blur-(--blur-md)`.

| Mode | Values | Switch |
| --- | --- | --- |
| Appearance | Light, Dark: every color holds both | `setColorScheme("dark")`, `set_color_scheme(..)`, `nsh.json` `"appearance"` |
| Accent | Cyan, Sky, Blue | `Themes.set("sky")`, `set_theme(RayXTheme::new(Accent::Sky, Density::Desktop))` |
| Density | Desktop, Touch | `Themes.set("cyan", "touch")`, `RayXTheme::new(.., Density::Touch)` |
| OS | Windows, macOS, GNOME, KDE | `Themes.set(.., .., "macos")`, `RayXTheme::of(Modes { os: Os::Macos, .. })` |

## Custom themes

A custom theme is a `RayXTheme` value, made from a base and the tokens it changes, and drawn with NSH's `set_theme`:

```rust
use rayx::components::prelude::*;

fn brand() -> RayXTheme {
    rayx_theme! {
        extends: RayXTheme::new(Accent::Blue, Density::Desktop),
        theme.colors.roles.primary.bg: color!("#7c3aed", "#8b5cf6"),
        theme.colors.roles.success: RoleColors::set(RoleSet::Green),
        platform.density.size.default.borders.radius: 10.0,
    }
}

set_theme(brand());
update_theme::<RayXTheme>(rayx_patch! { theme.colors.roles.primary.bg: color!("#e11d48") });
reset_theme::<RayXTheme>();
```

`color!` checks its hex colors at compile time; a path that is no token, or a value of the wrong type, fails to compile.
An app's `nsh.json` theme file and TSX's `updateTheme` set tokens by name (`"roles-primary-bg": "#7c3aed"`).

## Where it comes from

`crates/rayx_components/src/theme/tokens.rs` is generated from the Figma file's DTCG export (in RayX.Design) by the
`rayx-components` skill's `theme_codegen.py`: after an export, run `write` from RayX.Root, and `check` before committing.
Never edit it by hand.

## The gallery

[`examples/gallery`](../examples/gallery/README.md) is a mixed NSH app whose screens are TSX, so they reload as you save:
every component's Figma properties in a playground, its variants and sizes, and the Figma page's examples, with the
theme's modes in a toolbar. Run it with `cargo run -p rayx-cli -- app run -C examples/gallery`.
