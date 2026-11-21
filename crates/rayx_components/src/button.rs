//! `Button`: RayX's button (Figma `buttons/Button`), on nsh-base's `Button`.
//!
//! nsh-base's `Button` owns the behavior: presses, keyboard activation, focus, the disabled
//! state and the role screen readers hear. This file gives it RayX's look, from RayX's roles
//! only: the variant's intent (`intent-<variant>-…`), the size's level 5
//! (`spacing-<size>-lvl5-…`), its spacing, radius and `text-<size>-title1-bold` text style, and
//! the surface, interaction and border-width roles. Any theme that maps them draws it.

use nsh_core::element::Typography;
use nsh_core::prelude::*;

use crate::theme::roles::{
    BORDER_WIDTH_FOCUS_RING, BORDER_WIDTH_OUTLINE, INTERACTION_DISABLED_BG,
    INTERACTION_DISABLED_BORDER, INTERACTION_DISABLED_FG, INTERACTION_FOCUS_RING,
    INTERACTION_FOCUS_RING_DANGER, INTERACTION_FOCUS_RING_SUCCESS, SURFACE_BORDER, SURFACE_LINK,
    SURFACE_LINK_HOVER,
};
use crate::theme::{ComponentSize, IntentRoles, value};

/// A [`Button`]'s color and emphasis (Figma's `variant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum ButtonVariant {
    /// Secondary actions beside a primary one: the default.
    #[default]
    Neutral,
    /// The main action of a view; one per view.
    Primary,
    /// A destructive action. Pair it with a label that says what is destroyed: color alone does
    /// not signal it.
    Danger,
    /// Confirming a positive outcome.
    Success,
    /// A quiet action in toolbars and dense surfaces: no fill until hovered.
    Ghost,
    /// An action that reads like a link inside text or forms: no fill and no border.
    Link,
}

impl ButtonVariant {
    /// Every variant, in Figma's order.
    pub const ALL: [ButtonVariant; 6] = [
        ButtonVariant::Neutral,
        ButtonVariant::Primary,
        ButtonVariant::Danger,
        ButtonVariant::Success,
        ButtonVariant::Ghost,
        ButtonVariant::Link,
    ];
}

/// The props of [`Button`]: Figma's `buttons/Button` properties, with the same names and
/// defaults (`size`'s default is `default`; Figma's first variant, `small`, is no default).
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ButtonProps {
    /// Label text: a verb, one to three words, in sentence case ("Save changes"). It is also the
    /// accessible name.
    pub text: Prop<SharedString>,
    /// Color and emphasis: `neutral` (the default), `primary`, `danger`, `success`, `ghost` or
    /// `link`.
    #[prop(default)]
    pub variant: Prop<ButtonVariant>,
    /// Adds a `surface-border` border (`border-width-outline`) and keeps the variant's fill;
    /// disabled, the border is `interaction-disabled-border`. `link` draws no border.
    #[prop(default)]
    pub outlined: Prop<bool>,
    /// The size (level 5): `small` for dense toolbars and tables, `default` for most forms and
    /// dialogs, `large` for touch layouts and prominent calls to action.
    #[prop(default)]
    pub size: Prop<ComponentSize>,
    /// An unavailable action: no presses, no press feedback, the disabled colors; screen readers
    /// say it is dimmed. Figma's `state=disabled`.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// Shows `iconLeft` (it shows only when given).
    #[prop(default = true)]
    pub show_left_icon: Prop<bool>,
    /// The leading icon, drawn in a square of the level's `adornment-size` in the label's color (an SVG
    /// with `stroke="currentColor"` or `fill="currentColor"` follows it).
    pub icon_left: Prop<Option<Slot>>,
    /// Shows `iconRight` (it shows only when given).
    #[prop(default = true)]
    pub show_right_icon: Prop<bool>,
    /// The trailing icon, drawn like `iconLeft`.
    pub icon_right: Prop<Option<Slot>>,
    /// Makes the slot `slotLeft`: while it is off there is none (not shown, not made).
    #[prop(default)]
    pub show_slot_left: Prop<bool>,
    /// Content between the leading icon and the label, such as a Badge: the level's
    /// `adornment-size` high and at least as wide, centered and clipped.
    pub slot_left: Prop<Option<Slot>>,
    /// Makes the slot `slotRight`: while it is off there is none (not shown, not made).
    #[prop(default)]
    pub show_slot_right: Prop<bool>,
    /// Content between the label and the trailing icon, drawn like `slotLeft`.
    pub slot_right: Prop<Option<Slot>>,
}

/// The events of a [`Button`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum ButtonEvent {
    /// The button was pressed (by pointer, touch, or Enter or Space while focused).
    Press,
}

/// A labelled control that starts an action: saving, submitting, opening a dialog or running a
/// task. Its label (`text`) says what happens when it is pressed.
///
/// Use a `Link` to move to another page or site, a `ToggleButton` or `Switch` for an option
/// that stays on or off, an `IconButton` when there is room for an icon only, and a
/// `DropdownButton` for a menu of related actions. Use one `primary` button per view.
///
/// Its parts: the container, the label, the leading and trailing icons (`iconLeft`,
/// `iconRight`) and the leading and trailing slots (`slotLeft`, `slotRight`, such as a Badge).
/// Hover, press, keyboard focus and disabled are its states; `class` and `style` place it (a
/// width, a margin) and never restyle it.
///
/// # Rust
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_components::button::{ButtonVariant, button};
///
/// # fn save() {}
/// let saving = signal(false);
/// let _ = button()
///     .text("Save changes")
///     .variant(ButtonVariant::Primary)
///     .disabled(move || saving.get())
///     .on_press(move || save());
/// ```
#[component]
pub fn Button(props: ButtonProps, cx: Scope<ButtonEvent>) -> impl IntoElement {
    // One memo of the look: every binding below reads it, and runs again only when the theme
    // (a picked context, a patch, another document) or a prop it depends on changes it.
    let look = computed(move || {
        Look::of(
            props.variant.get(),
            props.size.get(),
            props.outlined.get(),
            props.disabled.get(),
        )
    });

    // Each icon is a box of the button's own, there while the app gives it and its `show*` prop
    // is on, with the icon centered in it. The icons are props set in code, not slots.
    let icon = move |slot: Prop<Option<Slot>>, show: Prop<bool>| {
        when(
            move || show.get() && slot.get().is_some(),
            move || {
                view()
                    .size(move || px(look.with(|l| l.adornment)))
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .child(slot.place(|s| s))
            },
        )
    };
    // `slotLeft` and `slotRight` are slots, marked with RayX's `Slot` (named by their JS names),
    // so a design tool finds them: a box the level's `adornment-size` high and at least as wide,
    // the content centered and clipped. Their `show*` prop makes them: while it is off there is
    // no slot. In the running app a slot is there only while filled; in design mode an empty one
    // is too, as `Slot`'s box at the adornment-size square, so the button keeps its height.
    let design = design_mode();
    let content = move |name: &'static str, slot: Prop<Option<Slot>>, show: Prop<bool>| {
        when(
            move || show.get() && (design || slot.get().is_some()),
            move || {
                let adornment = move || px(look.with(|l| l.adornment));
                crate::slot::slot()
                    .name(name)
                    .h(adornment)
                    .min_h(adornment)
                    .min_w(adornment)
                    // Empty (in design mode only): exactly the square, whatever `Slot` draws in it.
                    .style_with(move |s| {
                        if slot.get().is_none() {
                            s.w(adornment())
                        } else {
                            s
                        }
                    })
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(slot.place(|s| s))
            },
        )
    };

    // nsh-base's `Button`, by path: this module's `button()` is RayX's.
    nsh_base::button()
        .disabled(props.disabled)
        // The platforms send a disabled view no presses; one already on its way when `disabled`
        // turned on (or an accessibility action) is dropped here, so `onPress` never hears it.
        .on_press(move || {
            if !props.disabled.get_untracked() {
                cx.emit(ButtonEvent::Press);
            }
        })
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(move || px(look.with(|l| l.height)))
        .px(move || px(look.with(|l| l.inset)))
        .gap(move || px(look.with(|l| l.gap)))
        .rounded(move || px(look.with(|l| l.radius)))
        .border(move || px(look.with(|l| l.border_width)))
        .border_color(move || look.with(|l| l.border))
        .bg(move || look.with(|l| l.bg))
        // The label's text style and color, which the label and the icons inherit (an icon's
        // `currentColor` is its view's text color): the size's typography, whole and bound.
        .text_color(move || look.with(|l| l.fg))
        .typography(move || look.with(|l| l.text.clone()))
        .transition_colors()
        .duration(120.0_f32)
        .ease_out()
        .hover(|s| {
            s.bg(move || look.with(|l| l.bg_hover))
                .text_color(move || look.with(|l| l.fg_hover))
        })
        .active(|s| {
            s.bg(move || look.with(|l| l.bg_active))
                .text_color(move || look.with(|l| l.fg_hover))
        })
        .focus_visible(|s| {
            s.outline(move || px(look.with(|l| l.ring_width)))
                .outline_color(move || look.with(|l| l.ring))
        })
        .child(icon(props.icon_left, props.show_left_icon))
        .child(content("slotLeft", props.slot_left, props.show_slot_left))
        .child(text(props.text).number_of_lines(1u32))
        .child(content(
            "slotRight",
            props.slot_right,
            props.show_slot_right,
        ))
        .child(icon(props.icon_right, props.show_right_icon))
}

/// What a button draws with, for its variant, size, outline and disabled state: each field the
/// value of one of RayX's roles (`theme::value`), or a constant (transparent, 0). Public for the
/// crate's tests (`tests/button_roles.rs`), not API.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub bg: Hsla,
    pub bg_hover: Hsla,
    pub bg_active: Hsla,
    pub fg: Hsla,
    pub fg_hover: Hsla,
    pub border: Hsla,
    pub border_width: f32,
    pub ring: Hsla,
    pub ring_width: f32,
    pub height: f32,
    pub inset: f32,
    pub gap: f32,
    pub radius: f32,
    pub adornment: f32,
    pub text: Typography,
}

impl Look {
    /// The roles Figma's `buttons/Button` binds, for these props, read where the button is
    /// (reactive in a binding).
    pub fn of(variant: ButtonVariant, size: ComponentSize, outlined: bool, disabled: bool) -> Self {
        let sizes = size.roles();
        let none = Hsla::default();
        let intent = |r: IntentRoles| {
            (
                value(r.bg),
                value(r.bg_hover),
                value(r.bg_active),
                value(r.text),
                value(r.text),
            )
        };
        let neutral = IntentRoles::NEUTRAL;
        let (bg, bg_hover, bg_active, fg, fg_hover) = match variant {
            ButtonVariant::Neutral => intent(neutral),
            ButtonVariant::Primary => intent(IntentRoles::PRIMARY),
            ButtonVariant::Danger => intent(IntentRoles::DANGER),
            ButtonVariant::Success => intent(IntentRoles::SUCCESS),
            ButtonVariant::Ghost => (
                none,
                value(neutral.bg_hover),
                value(neutral.bg_active),
                value(neutral.text),
                value(neutral.text),
            ),
            ButtonVariant::Link => (
                none,
                none,
                none,
                value(SURFACE_LINK),
                value(SURFACE_LINK_HOVER),
            ),
        };
        let ring = value(match variant {
            ButtonVariant::Danger => INTERACTION_FOCUS_RING_DANGER,
            ButtonVariant::Success => INTERACTION_FOCUS_RING_SUCCESS,
            _ => INTERACTION_FOCUS_RING,
        });
        let outlined = outlined && variant != ButtonVariant::Link;
        let border_width = if outlined {
            value(BORDER_WIDTH_OUTLINE)
        } else {
            0.0
        };
        let level = sizes.level(5);
        let mut look = Look {
            bg,
            bg_hover,
            bg_active,
            fg,
            fg_hover,
            border: value(SURFACE_BORDER),
            border_width,
            ring,
            ring_width: value(BORDER_WIDTH_FOCUS_RING),
            height: value(level.height),
            inset: value(level.inset),
            gap: value(sizes.content),
            radius: value(sizes.radius),
            adornment: value(level.adornment_size),
            text: value(sizes.text.title1_bold),
        };
        if disabled {
            let fill = match variant {
                ButtonVariant::Ghost | ButtonVariant::Link => none,
                _ => value(INTERACTION_DISABLED_BG),
            };
            look.bg = fill;
            look.bg_hover = fill;
            look.bg_active = fill;
            look.fg = value(INTERACTION_DISABLED_FG);
            look.fg_hover = value(INTERACTION_DISABLED_FG);
            look.border = value(INTERACTION_DISABLED_BORDER);
        }
        look
    }
}
