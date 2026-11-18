# Components and theme

`rayx_components` (`rayx::components` in the umbrella) is the RayX design system in code: the Figma file's tokens as an
NSH theme document read through typed keys, and its components, each the Figma component of the same name on the NSH base component it mirrors.
Components are written in Rust and exported to TSX with typed props. The full guide for each component (props,
examples, DO and DON'T) is the `rayx-components` skill in RayX.Root (`.claude/skills/rayx-components/`).

| Component | Figma | Base |
| --- | --- | --- |
| `Button` | `buttons/Button` | nsh-base `Button` |

## Use it

The library is in the app's UI set: `#[rayx::native_module]` registers it with every RayX TS class. It adds the
components and the theme document (RayX's tokens become the app's theme: its classes, `token()`, the modifiers
`setModifier` picks and the roles NSH's own parts draw with):

```rust
use rayx::prelude::*;

#[rayx::native_module("my-app/native")]
fn native(registry: &mut Registry) {}
```

A hand-written `#[native_module]` registers it with `rayx::components::register`:

```rust
use nsh_core::prelude::*;

#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    rayx::components::register(registry);
}
```

TSX imports them by the set's specifier; props are Figma's properties in camel case:

```tsx
import { Button } from "my-app/native";

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

RayX's tokens are an NSH **theme document**, `crates/rayx_components/theme.json`: every token declared once (its kind,
its path, its Figma description), its values in Light and Dark, the modes as **modifiers**, and the roles NSH's own
parts draw with. `register` registers it, so classes, `token()`, theme files and nsh-base read RayX's tokens.

Rust reads a token through its **typed key** (`rayx::components::theme::keys`), at the token's path, its Figma WEB code
syntax: `theme/colors/roles/primary/bgHover` is `keys::theme::colors::roles::primary::BG_HOVER`,
`platform/density/size/default/lvl5/height` is `keys::platform::density::size::default::lvl5::HEIGHT`. `key.get()`
(or `value(key)`, the kind's default when missing) inside a binding follows every change of the token: the appearance, a
modifier, the app's tokens. `ComponentSize::keys()`, `SizeKeys::level(n)`, `RoleKeys` and `TextStyleKeys` pick keys by
a component's size, level and role.

```rust
use nsh_core::prelude::*;
use rayx::components::prelude::*;

view()
    .bg(move || value(keys::theme::colors::surfaces::PANEL1))
    .p(move || px(value(keys::platform::density::insets::INSET5).0))
    .child(bind_text_style(text("Settings"), text_style_keys!(keys::platform::density::typography::h3)))
```

In TSX the tokens are classes: the token's path after its collection in kebab case behind its kind's namespace:
`bg-surfaces-app`, `text-roles-primary-text`, `gap-gap4`, `p-insets-inset5`, `h-size-default-lvl5-height`,
`rounded-borders-radius4`, `shadow-default`. Line heights and letter spacing are points, through `var()`:
`leading-(--spacing-typography-body2-line-height)`, `tracking-(--spacing-typography-body2-letter-spacing)`. So are the
shadows and blurs named as Tailwind's sizes (`sm`, `md`, `lg`, `xl`), whose classes stay Tailwind's:
`shadow-(--shadow-md)`, `blur-(--blur-md)`. `token("roles-primary-bg")` reads one by name.

| Mode | Values | Switch |
| --- | --- | --- |
| Appearance | Light, Dark: every color holds both, in every context | `setColorScheme("dark")`, `set_color_scheme(..)`, `nsh.json` `"appearance"` |
| Accent | cyan, sky, blue | `setModifier("accent", "sky")`, `Accent::Sky.set()` |
| Density | desktop, touch | `setModifier("density", "touch")`, `Density::Touch.set()` |
| OS | windows, macos, gnome, kde | `setModifier("os", "macos")`, `Os::Macos.set()` |

Each modifier starts at the document's default (cyan, desktop, windows) on every platform: RayX sets none itself, the
app picks them. For one part of the app, `<ThemeScope modifiers={{ accent: "blue" }}>` or
`theme_scope().modifiers(Some(ScopeModifiers::from(Accent::Blue)))` (build what it holds where it places it: a view
built before is outside the scope). `modifier("accent")` and `Accent::current()` say what is picked.

## Custom themes

A custom theme is NSH's: the app's tokens over RayX's, by key or by name.

```rust
use nsh_core::prelude::*;
use rayx::components::prelude::*;

let primary = keys::theme::colors::roles::primary::BG;
update_tokens(Tokens::default().with(primary, color!("#7c3aed", "#8b5cf6")));
reset_tokens([primary.name()]); // RayX's again
```

`color!` checks its hex colors at compile time (`color!("#e11d48")` for both appearances). An app's `nsh.json` theme
file and TSX's `updateTheme` set tokens by name (`"roles-primary-bg": "#7c3aed"`), and `ThemeScope` gives a subtree a
theme of its own.

## Where it comes from

`crates/rayx_components/theme.json` is generated from the Figma file's DTCG export (in RayX.Design) by the
`rayx-components` skill's `theme_codegen.py`: after an export, run `write` from RayX.Root, and `check` before committing.
Never edit it by hand; `theme_keys!` makes the keys and the modifier enums from it when the crate builds.

## The gallery

[`examples/gallery`](../examples/gallery/README.md) is a mixed NSH app whose screens are TSX, so they reload as you save:
every component's Figma properties in a playground, its variants and sizes, and the Figma page's examples, with the
theme's modes in a toolbar. Run it with `cargo run -p rayx-cli -- app -C examples/gallery`. run
