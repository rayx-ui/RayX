//! `Slot`: a place in a component that takes content from outside.
//!
//! A component marks each of its slots with a `Slot`, named as the prop that fills it, around
//! what goes there: `<Slot name="footer">{props.footer}</Slot>` in TSX,
//! `slot().name("footer").child(props.footer.place(|f| f))` in a Rust view. A design tool finds
//! the component's slots by it, in TSX and in a Rust view alike (it tells NSH's design mode where
//! it is: `design_slot`), and what the component's own code puts in it is the slot's
//! default content (`{props.footer ?? <Button text="Close" />}`): what a user of the component
//! passes replaces it.
//!
//! A slot holds one component instance, or nothing. Empty, it shows nothing in the running app
//! and takes no space (no `gap` of its parent's either). In design mode only (a design tool
//! started the app: NSH's `design_mode()`), an empty slot draws a box a design tool can drop
//! into, in RayX's roles. Two or more things given to one slot are a mistake of the code that
//! gives them: a dev build warns once, naming the slot, and shows them all (several things go in
//! an `AutoLayout` put into the slot).

use std::sync::Mutex;

use nsh_core::element::Typography;
use nsh_core::prelude::*;

use crate::auto_layout::ParentLayout;
use crate::theme::roles::{
    BORDER_WIDTH_OUTLINE, INTENT_PRIMARY_ACCENT, INTENT_PRIMARY_BG_TRANSLUCENT, RADIUS_DEFAULT,
    SPACING_DEFAULT_LVL5_HEIGHT, SPACING_SMALL_LVL5_INSET, SURFACE_TEXT_SUBTLE, TEXT_SMALL_REGULAR,
};
use crate::theme::value;

/// The props of [`Slot`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SlotProps {
    /// The slot's name: the name of the component's prop that fills it (`footer`), which a
    /// design tool shows and fills it by.
    pub name: Prop<SharedString>,
    /// What the slot holds: one component instance (the prop that fills it, or the component's
    /// default content), or nothing.
    pub children: Prop<Children>,
}

/// A place in a component that takes content from outside: one component instance, or nothing.
///
/// A component marks each of its slots with it, named as the prop that fills the slot, around
/// what goes there. What the component's own code puts in it is the slot's default content;
/// what a user of the component passes replaces it:
///
/// ```tsx
/// <Slot name="footer">{props.footer ?? <Button text="Close" />}</Slot>
/// ```
///
/// Empty, it shows nothing in the running app and takes no space, not even its parent's `gap`.
/// In design mode only (NSH's `design_mode()`: a design tool started the app) an empty slot is a
/// box to drop into: the `intent-primary-bg-translucent` tint with a dashed
/// `border-width-outline` edge in `intent-primary-accent`, `radius-default` corners, at least
/// `spacing-default-lvl5-height` on each side, and its name in `text-small-regular`
/// (`surface-text-subtle`) inset by `spacing-small-lvl5-inset`.
///
/// A component that sizes the place (`slot().name("slotLeft").size(..)`) gets the empty box at
/// that size: the component's styles on the slot come after the slot's own.
///
/// Holding one element, it is a box around it laid out as it is: the element keeps its size and
/// place. Two or more things in one slot are a mistake: a dev build warns once, naming the slot,
/// and shows them all. Put several things in an `AutoLayout`, and that in the slot.
///
/// # Rust
///
/// The type is named by its path in a module that also uses NSH's prelude, whose `Slot` is the
/// type of a slot prop (`Prop<Option<Slot>>`):
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_components::slot::slot;
///
/// #[derive(Debug, Clone, PartialEq, Props)]
/// pub struct CardProps {
///     /// The card's footer.
///     pub footer: Prop<Option<Slot>>,
/// }
///
/// #[component]
/// pub fn Card(props: CardProps) -> impl IntoElement {
///     view().child(
///         slot()
///             .name("footer")
///             .child(props.footer.place(|f| f))
///             // The default content, while the app passes none.
///             .child(when(move || props.footer.get().is_none(), || text("No footer"))),
///     )
/// }
/// # fn main() {}
/// ```
#[component]
pub fn Slot(props: SlotProps) -> impl IntoElement {
    // Where it is, for a design tool (nothing without NSH's dev tools).
    design_slot(props.name);
    let design = design_mode();
    let empty = move || props.children.get().is_empty();
    // The empty box's look, read only in design mode.
    let look = computed(move || design.then(Look::of));
    // What it holds is not laid out against an `AutoLayout` around the slot: the slot is the
    // box around it.
    provide_context(ParentLayout(None));

    view()
        .style_with(move |s| {
            if !empty() {
                return s;
            }
            match look.get() {
                // Empty in the running app: out of the layout, so no gap is taken for it.
                None => s.hidden(),
                Some(look) => s
                    .min_w(px(look.size))
                    .min_h(px(look.size))
                    .px(px(look.inset))
                    .items_center()
                    .justify_center()
                    .rounded(px(look.radius))
                    .border(px(look.border_width))
                    .border_dashed()
                    .border_color(look.border)
                    .bg(look.fill),
            }
        })
        .children_each(props.children, move |child, at| {
            // Two or more things given to the slot: a dev build says so, once per slot name.
            if cfg!(debug_assertions) {
                effect(move || {
                    let count = at.count();
                    if count > 1 {
                        untracked(|| warn_several(&props.name.get(), count));
                    }
                });
            }
            child
        })
        .child(when(
            move || design && empty() && !props.name.get().is_empty(),
            move || {
                text(props.name)
                    .number_of_lines(1u32)
                    .typography(move || {
                        look.with(|l| l.as_ref().map(|l| l.text.clone()).unwrap_or_default())
                    })
                    .text_color(move || {
                        look.with(|l| l.as_ref().map(|l| l.label).unwrap_or_default())
                    })
            },
        ))
}

/// What an empty slot draws in design mode: each field the value of one of RayX's roles
/// (`theme::value`). Public for the crate's tests (`tests/slot_design.rs`), not API.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// `intent-primary-bg-translucent`.
    pub fill: Hsla,
    /// `intent-primary-accent`, dashed.
    pub border: Hsla,
    /// `border-width-outline`.
    pub border_width: f32,
    /// `radius-default`.
    pub radius: f32,
    /// `spacing-default-lvl5-height`: the least width and height.
    pub size: f32,
    /// `spacing-small-lvl5-inset`: the name's inset from the left and right edges.
    pub inset: f32,
    /// `text-small-regular`: the name's text style.
    pub text: Typography,
    /// `surface-text-subtle`: the name's color.
    pub label: Hsla,
}

impl Look {
    /// The roles an empty slot reads, where it is (reactive in a binding).
    pub fn of() -> Self {
        Look {
            fill: value(INTENT_PRIMARY_BG_TRANSLUCENT),
            border: value(INTENT_PRIMARY_ACCENT),
            border_width: value(BORDER_WIDTH_OUTLINE),
            radius: value(RADIUS_DEFAULT),
            size: value(SPACING_DEFAULT_LVL5_HEIGHT),
            inset: value(SPACING_SMALL_LVL5_INSET),
            text: value(TEXT_SMALL_REGULAR),
            label: value(SURFACE_TEXT_SUBTLE),
        }
    }
}

/// The slots a warning was given for, by name: each is named once per process.
static WARNED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Warns, once per slot name, that slot `name` was given `count` things.
fn warn_several(name: &str, count: usize) {
    let mut warned = WARNED.lock().unwrap_or_else(|e| e.into_inner());
    if warned.iter().any(|n| n == name) {
        return;
    }
    warned.push(name.to_owned());
    eprintln!(
        "rayx: the slot `{name}` was given {count} things; a slot holds one component \
         (put several in an AutoLayout, and that in the slot)"
    );
}

/// The names of the slots a dev build warned of so far (two or more things given to one), in
/// the order it warned. Public for the crate's tests (`tests/slot_view.rs`), not API.
#[doc(hidden)]
pub fn warned() -> Vec<String> {
    WARNED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
