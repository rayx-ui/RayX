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
| `AutoLayout` | a frame with auto layout | a view ([below](#autolayout)) |
| `Slot` | a slot | a view ([below](#slot)) |

## Use it

The library is a UI set of its own, `rayx/components` (`rayx::components::set()`, its specifier
`rayx::components::SPECIFIER`): the components, the previews' types (`Previews<C>`, [Previews](previews.md)), RayX's
roles (`role()` and role-named classes read them, and NSH checks every theme against them) and RayX's default theme,
the app's theme unless it has one of its own. Every app's set serves it: `#[rayx::native_module]` does, so nothing
names it in `nsh.json` and the app's set registers none of it:

```rust
use rayx::prelude::*;

#[rayx::native_module("my-app/native")]
fn native(registry: &mut Registry) {}
```

A hand-written `#[native_module]` serves it with `Registry::serve`:

```rust
use nsh_core::prelude::*;

#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    registry.serve(rayx::components::set());
}
```

TSX imports them from `rayx/components`, in every app, and so do the library's own preview files; props are Figma's
properties in camel case (a fragment: `save`, `Icon` and `Badge` are the app's own):

<!-- tsx: fragment -->
```tsx
import { Button } from "rayx/components";

const saveButton = <Button text="Save changes" variant="primary" onPress={save} />;
const inboxButton = (
  <Button text="Inbox" iconLeft={<Icon name="inbox" />} showSlotRight slotRight={<Badge count={3} />} />
);
```

An app's set that registers a component of the library itself is refused when the app is described (NSH names both
sets): serve the library's set instead. Two crates of one app serving it serve it once.

Rust views use the constructors; props are Figma's properties in snake case:

```rust
use rayx::components::prelude::*;

button().text("Save changes").variant(ButtonVariant::Primary).on_press(move || save())
```

`class` and `style` on a component place it (a margin, a width); they never restyle it. A look the library lacks is a new
variant in the library and in Figma.

## AutoLayout

`AutoLayout` is RayX's container for several things, laid out as Figma's auto layout lays out a frame. It is a thin
wrapper: one view whose flex properties its props set, holding its children, any number of them, as they are. Its
props are Figma's auto layout properties:

| Prop | Values | Figma |
| --- | --- | --- |
| `direction` | `vertical` (default), `horizontal` | direction |
| `gap` | `none` (default), `gap1` … `gap8`: the role `spacing-gap<n>` | gap between items |
| `spaceBetween` | `true`: the children spread along the direction, `gap` the least space between them | spacing "Auto" |
| `padding` | `none` (default), `inset1` … `inset8`: the role `spacing-inset<n>`, on every side | padding |
| `paddingTop`, `paddingRight`, `paddingBottom`, `paddingLeft` | as `padding`, for one side, over `padding` | padding per side |
| `align` | `topLeft` (default), `top`, `topRight`, `left`, `center`, `right`, `bottomLeft`, `bottom`, `bottomRight` | alignment |
| `wrap`, `wrapGap` | wraps onto further lines; the space between lines (`gap`'s when unset) | wrap, spacing between rows |
| `width`, `height` | `hug` (default), `fill`, `fixed` (at `fixedWidth`, `fixedHeight`, in points) | resizing |

The gap and the padding are spacing roles, never numbers, so any theme that maps the roles spaces it. With
`spaceBetween`, `align` places the children across the direction only. Left and right are the start and the end in a
right-to-left layout.

<!-- tsx: fragment -->
```tsx
import { AutoLayout, Button } from "rayx/components";

const actions = (
  <AutoLayout direction="horizontal" gap="gap2" padding="inset3" align="right" width="fill">
    <Button text="Cancel" />
    <Button text="Save" variant="primary" />
  </AutoLayout>
);
```

```rust
use rayx::components::prelude::*;

auto_layout()
    .direction(AutoLayoutDirection::Horizontal)
    .gap(SpacingGap::Gap2)
    .padding(SpacingInset::Inset3)
    .child(button().text("Cancel"))
    .child(button().text("Save").variant(ButtonVariant::Primary))
```

**A child's sizing.** In Figma, hug, fill and fixed are each child's own, read against the auto layout around it. JSX
has no prop of a parent per child, so here they stay each child's:

- An `AutoLayout` child sizes itself with its own `width` and `height`, read against the `AutoLayout` around it, which
  provides its direction to its children as context: `fill` along that direction takes the space the other children
  leave (`flex: 1`), across it the whole breadth (`align-self: stretch`); `fixed` is `fixedWidth` / `fixedHeight`;
  `hug` is its content's size. Outside an `AutoLayout` the parent is taken for a column, as a view is.
- Any other child (a `Button`, an app's component) hugs, at its natural size. Its `class` (in Rust, its styles), which
  places a component without restyling it, makes it fill or fixes it: `flex-1` fills along the `AutoLayout`'s
  direction, `self-stretch` across it, `w-[120px]` / `h-[40px]` fix it. A design tool writes these from the direction
  it reads in the `AutoLayout`'s props.

Fill acts in the `AutoLayout` directly around the child: a `Slot` between them is a box of its own, and provides no
direction.

## Slot

A slot is a place in a component that takes content from outside. A component marks each of its slots with `Slot`,
named as the prop that fills it, around what goes there; a design tool finds the component's slots by it. A slot holds
one component instance, or nothing; several things go into a slot through an `AutoLayout` put into it. What the
component's own code puts into the slot is its default content, and what the component's user passes replaces it:

<!-- tsx: fragment -->
```tsx
import { Text, type JSX } from "nsh";
import { AutoLayout, Button, Slot } from "rayx/components";

function Dialog(props: { title: string; footer?: JSX.Element }) {
  return (
    <AutoLayout gap="gap4" padding="inset5">
      <Text>{props.title}</Text>
      <Slot name="footer">{props.footer ?? <Button text="Close" />}</Slot>
    </AutoLayout>
  );
}
```

In a Rust view, `slot()` makes it (its type is `rayx::components::slot::Slot`, by its path: NSH's prelude has a `Slot`
too, the type of a slot prop):

```rust
use rayx::components::prelude::*;

slot()
    .name("footer")
    .child(props.footer.place(|f| f))
    .child(when(move || props.footer.get().is_none(), || button().text("Close")))
```

- **Empty**, it shows nothing in the running app and takes no space: it is out of the layout, so its parent's `gap`
  is not taken for it either, and it comes back when content comes.
- **In design mode only** (NSH's `design_mode()`: a design tool started the app), an empty slot draws a box a design
  tool can drop into, from roles: the `intent-primary-bg-translucent` tint, a dashed `border-width-outline` edge in
  `intent-primary-accent`, `radius-default` corners, at least `spacing-default-lvl5-height` each way, and the slot's
  name in `text-small-regular` and `surface-text-subtle`, inset by `spacing-small-lvl5-inset`. A component that sizes
  the place (styles on the `slot()` it draws) gets the box at that size: its styles come after the slot's own.
- **A design tool finds it** in a Rust view as in TSX: `Slot` tells NSH's design mode that it is the place of the slot
  of its name, of the component that drew it (NSH's `design_slot`, a dev build's only), so the slot is listed under
  that component, empty or filled. Name it as the prop that fills it in JS (`slotLeft`, not `slot_left`). A slot the
  component's code makes only on a condition (`Button`'s `showSlotLeft`) is not there while the condition is off.
- **Holding one element**, it is a box around that element, laid out as it is without the slot.
- **Two or more things** in one slot are a mistake of the code that gives them: a dev build warns once, naming the
  slot, and shows them all.
- **A slot's props through it.** The props and press handlers a component places on its slot (a trigger's) reach
  the one element inside a `Slot`: NSH looks through a component whose root has only styles of its own and places
  its children, while it holds one element.

## Previews and files

Each component ships its previews beside its source, `crates/rayx_components/src/<name>.preview.tsx`
(`button.preview.tsx`): plain data typed with `Previews<C>`, importing the component and the type from
`rayx/components`, with an `id` unique in the crate and literal props, so a design tool shows RayX's components in their
states with nothing written by the app (see [Previews](previews.md)). A new component lands with its preview file;
`crates/rayx_components/tests/previews.rs` checks that every component the set registers has one, literal, and that
each state renders.

A file the library ships (an icon, a font) goes in the crate's `assets/` and is listed by its set
(`registry.assets(assets!())` in `set()`): TSX imports it checked at build time
(`import star from "asset:/@rayx_components/icons/star.svg"`), and RayX's [asset service](services/assets.md) lists it
in the catalog without the app naming the package. The library ships none yet.

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

**Moving from an older RayX theme:** `nsh theme convert` is only for a complete theme of the app's own, which must
then map RayX's roles; an app that only tweaked some of RayX's tokens moves those values into a patch keyed by the new
token names (the DTCG path in kebab case, `theme-colors-intents-primary-bg`), or re-points a RayX role in the patch's
`roles`.

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
