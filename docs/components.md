# Components and theme

`rayx_components` (`rayx::components` in the umbrella) is the RayX design system in code: the **roles** its components
read (the design system's semantic layer), RayX's **default theme** (an NSH theme document that maps them onto the
design system's tokens), and its components, each the Figma component of the same name on the NSH base component it
mirrors.
Components are written in Rust and exported to TSX with typed props. The full guide for each component (props,
examples, DO and DON'T) is the `rayx-components` skill in RayX.Root (`.claude/skills/rayx-components/`).

| Component | Figma | Base |
| --- | --- | --- |
| `Button` | `buttons/Button` | nsh-base `Button` |

## Use it

The library is in the app's UI set: `#[rayx::native_module]` registers it with every RayX TS class. It adds the
components, RayX's roles (`role()` and role-named classes read them, and NSH checks every theme against them) and
RayX's default theme, the app's theme unless it has one of its own:

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

TSX imports them by the set's specifier; props are Figma's properties in camel case (a fragment: `save`, `Icon` and
`Badge` are the app's own):

<!-- tsx: fragment -->
```tsx
import { Button } from "my-app/native";

const saveButton = <Button text="Save changes" variant="primary" onPress={save} />;
const inboxButton = (
  <Button text="Inbox" iconLeft={<Icon name="inbox" />} showSlotRight slotRight={<Badge count={3} />} />
);
```

Rust views use the constructors; props are Figma's properties in snake case:

```rust
use rayx::components::prelude::*;

button().text("Save changes").variant(ButtonVariant::Primary).on_press(move || save())
```

`class` and `style` on a component place it (a margin, a width); they never restyle it. A look the library lacks is a new
variant in the library and in Figma.

## The theme

RayX's components read **roles**, never tokens: names such as `intent-primary-bg`, `spacing-default-lvl5-height` or
`text-default-title1-bold`. They are declared once in `crates/rayx_components/roles.json` (name, value type, doc, and
`required` for the roles a component reads). A **theme** is any NSH theme document that maps them onto tokens of its
own; its modes are its **modifiers**.

Rust reads a role through its typed constant, `rayx::components::theme::roles` (generated from `roles.json`:
`roles::INTENT_PRIMARY_BG_HOVER`, `roles::SPACING_DEFAULT_LVL5_HEIGHT`), with `value(role)`: inside a binding it
follows every change (a picked context, a patch, another theme), and it gives the type's default when the theme maps
the role onto nothing. `IntentRoles` and `ComponentSize::roles()` (`SizeRoles`, `level(n)`) pick roles by a
component's variant, size and level.

```rust
use nsh_core::prelude::*;
use rayx::components::prelude::*;

view()
    .bg(move || value(roles::SURFACE_PANEL1))
    .p(move || px(value(roles::SPACING_INSET5)))
    .child(text("Settings").typography(move || value(roles::TEXT_H3)))
```

In TSX, classes name roles: a color role by its name, a namespaced role without its namespace's prefix, a text style
by its typography role: `bg-surface-app`, `text-intent-primary-text`, `gap-gap4`, `p-inset5`, `h-default-lvl5-height`,
`rounded-default`, `rounded-4`, `text-default-regular` (family, size, weight, line height and letter spacing at once),
`shadow-md`. `role("intent-primary-bg")` reads one by name.

**RayX's default theme**, `crates/rayx_components/theme.json`, names its tokens by their DTCG paths
(`theme-colors-surfaces-app`) and has the design system's modes as modifiers:

| Modifier | Contexts | Picked by |
| --- | --- | --- |
| `colorMode` | light, dark | the theme service: following the system's light or dark, or the app's choice |
| `accent` | cyan, sky, blue | the theme service's `set_context("accent", "sky")` (TS `setContext`), or NSH's `set_modifier` |
| `density` | desktop, touch | `set_context("density", "touch")`, or `set_modifier` |
| `os` | windows, macos, gnome, kde | `set_context("os", "macos")`, or `set_modifier` |

RayX sets no modifier itself. With the [theme service](services/theme.md) (the `rayx` feature `theme`, `add_theme` /
`addTheme()`), light and dark follow the system's color scheme or the app's choice. Without it, the theme shows
`colorMode`'s default, light, and the app picks contexts with `set_modifier`. For one part of the app,
`<ThemeScope modifiers={{ accent: "blue" }}>`. `modifier("accent")` says what is picked.

## Another theme, and tweaks

- **An app's own document** (`nsh.json`'s `"theme"`, `Root::theme`, or `set_theme` / `setTheme` at run time) replaces
  RayX's: it has tokens of its own and maps RayX's roles onto them (each role onto a semantic token that references
  its palette). The gallery's [sample theme](../examples/gallery/themes/sample.json) is one.
- **A patch over RayX's theme** (`nsh.json`'s `themeOverrides`, `update_tokens(patch)`, `updateTheme(patch)`)
  changes some of its values by the default theme's token names, or re-points a role in its `roles`. A literal value
  is the same in every mode; a value that references a token following the modes, or a role re-pointed onto another
  semantic token, keeps a value per mode:

```rust
use nsh_core::element::{Patch, PatchValues, update_tokens};

update_tokens(
    Patch::new()
        .value("platform-density-size-default-borders-radius", 10.0_f32) // every mode
        .role("interaction-focus-ring", "theme-colors-intents-primary-bg"), // follows the modes
);
```

The `rayx-components` skill's `references/theming.md` has the details and the examples.

## Where it comes from

`crates/rayx_components/theme.json` is RayX.Dtcg's export of the design system's DTCG files (in RayX.Design) with the
role map (`tokens/dtcg/rayx.role-map.json`, which names the token of every role) and `roles.json`, run by the
`rayx-components` skill's `theme_export.py`: after a change to any of them, run `write` from RayX.Root, and `check`
before committing. Never edit it by hand. `build.rs` makes `theme::roles` from `roles.json` when the crate builds.

## The gallery

[`examples/gallery`](../examples/gallery/README.md) is a mixed NSH app whose screens are TSX, so they reload as you save:
every component's Figma properties in a playground, its variants and sizes, and the Figma page's examples, with a
toolbar that switches the theme document (RayX's or the sample theme), the color mode and each modifier's context. Run it with `cargo run -p rayx-cli -- app -C examples/gallery`. run
