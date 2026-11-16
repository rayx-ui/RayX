//! `Button`: RayX's button (Figma `buttons/Button`), on nsh-base's `Button`.
//!
//! nsh-base's `Button` owns the behavior: presses, keyboard activation, focus, the disabled
//! state and the role screen readers hear. This file gives it RayX's look, from the theme's
//! typed keys: the variant's role colors (`theme.colors.roles.<role>`), the size's level 5
//! (`platform.density.size.<size>.lvl5`), its spacing, radius and `title1Bold` text style.

use nsh_core::prelude::*;

use crate::theme::keys::platform::density::borders;
use crate::theme::keys::theme::colors::{interaction, surfaces};
use crate::theme::{ComponentSize, RoleKeys, value, weight};

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
    /// Adds a `surfaces/border` border (`borders/outlineWidth`) and keeps the variant's fill;
    /// disabled, the border is `interaction/disabledBorder`. `link` draws no border.
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
    /// The leading icon, drawn in a square of `lvl5/adornmentSize` in the label's color (an SVG
    /// with `stroke="currentColor"` or `fill="currentColor"` follows it).
    pub icon_left: Prop<Option<Slot>>,
    /// Shows `iconRight` (it shows only when given).
    #[prop(default = true)]
    pub show_right_icon: Prop<bool>,
    /// The trailing icon, drawn like `iconLeft`.
    pub icon_right: Prop<Option<Slot>>,
    /// Shows `slotLeft`.
    #[prop(default)]
    pub show_slot_left: Prop<bool>,
    /// Content between the leading icon and the label, such as a Badge: `lvl5/adornmentSize`
    /// high and at least as wide, centered and clipped.
    pub slot_left: Prop<Option<Slot>>,
    /// Shows `slotRight`.
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
    // or a prop it depends on changes it.
    let look = computed(move || {
        Look::of(
            props.variant.get(),
            props.size.get(),
            props.outlined.get(),
            props.disabled.get(),
        )
    });
    // A weight and an outline width are no bindings in NSH: read once, from the tokens the
    // button is made with (the same for every size and density in the export).
    let at_rest = props.size.get_untracked().keys();
    let (font_weight, ring_width) = untracked(|| {
        (
            weight(value(at_rest.text.title1_bold.font_weight)),
            value(borders::FOCUS_RING_WIDTH),
        )
    });

    // Each icon and slot is a box of the button's own, there while the app fills the slot and
    // its `show*` prop is on, with the app's content centered in it. The box is a view of this
    // component, not the slot's own node: a slot TSX fills does not mount when it is placed
    // straight into a nested component's content (`nsh_base::button()`'s children).
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
    let content = move |slot: Prop<Option<Slot>>, show: Prop<bool>| {
        when(
            move || show.get() && slot.get().is_some(),
            move || {
                view()
                    .h(move || px(look.with(|l| l.adornment)))
                    .min_w(move || px(look.with(|l| l.adornment)))
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
        // `currentColor` is its view's text color).
        .text_color(move || look.with(|l| l.fg))
        .text_size(move || px(look.with(|l| l.font_size)))
        .line_height(move || px(look.with(|l| l.line_height)))
        .letter_spacing(move || px(look.with(|l| l.letter_spacing)))
        .font_weight(font_weight)
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
            s.outline(px(ring_width))
                .outline_color(move || look.with(|l| l.ring))
        })
        .child(icon(props.icon_left, props.show_left_icon))
        .child(content(props.slot_left, props.show_slot_left))
        .child(text(props.text).number_of_lines(1u32))
        .child(content(props.slot_right, props.show_slot_right))
        .child(icon(props.icon_right, props.show_right_icon))
}

/// What a button draws with, for its variant, size, outline and disabled state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Look {
    pub(crate) bg: AdaptiveColor,
    pub(crate) bg_hover: AdaptiveColor,
    pub(crate) bg_active: AdaptiveColor,
    pub(crate) fg: AdaptiveColor,
    pub(crate) fg_hover: AdaptiveColor,
    pub(crate) border: AdaptiveColor,
    pub(crate) border_width: f32,
    pub(crate) ring: AdaptiveColor,
    pub(crate) height: f32,
    pub(crate) inset: f32,
    pub(crate) gap: f32,
    pub(crate) radius: f32,
    pub(crate) adornment: f32,
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) letter_spacing: f32,
}

impl Look {
    /// The tokens Figma's `buttons/Button` binds, for these props, read where the button is
    /// (reactive in a binding).
    pub(crate) fn of(
        variant: ButtonVariant,
        size: ComponentSize,
        outlined: bool,
        disabled: bool,
    ) -> Self {
        let sizes = size.keys();
        let none = AdaptiveColor::TRANSPARENT;
        let role = |r: RoleKeys| {
            (
                value(r.bg),
                value(r.bg_hover),
                value(r.bg_active),
                value(r.text),
                value(r.text),
            )
        };
        let neutral = RoleKeys::NEUTRAL;
        let (bg, bg_hover, bg_active, fg, fg_hover) = match variant {
            ButtonVariant::Neutral => role(neutral),
            ButtonVariant::Primary => role(RoleKeys::PRIMARY),
            ButtonVariant::Danger => role(RoleKeys::DANGER),
            ButtonVariant::Success => role(RoleKeys::SUCCESS),
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
                value(surfaces::LINK),
                value(surfaces::LINK_HOVER),
            ),
        };
        let ring = value(match variant {
            ButtonVariant::Danger => interaction::FOCUS_RING_DANGER,
            ButtonVariant::Success => interaction::FOCUS_RING_SUCCESS,
            _ => interaction::FOCUS_RING,
        });
        let outlined = outlined && variant != ButtonVariant::Link;
        let border_width = if outlined {
            value(borders::OUTLINE_WIDTH)
        } else {
            0.0
        };
        let level = sizes.level(5);
        let text = sizes.text.title1_bold;
        let mut look = Look {
            bg,
            bg_hover,
            bg_active,
            fg,
            fg_hover,
            border: value(surfaces::BORDER),
            border_width,
            ring,
            height: value(level.height).0,
            inset: value(level.inset).0,
            gap: value(sizes.content).0,
            radius: value(sizes.radius).0,
            adornment: value(level.adornment_size).0,
            font_size: value(text.font_size).0,
            line_height: value(text.line_height).0,
            letter_spacing: value(text.letter_spacing).0,
        };
        if disabled {
            let fill = match variant {
                ButtonVariant::Ghost | ButtonVariant::Link => none,
                _ => value(interaction::DISABLED_BG),
            };
            look.bg = fill;
            look.bg_hover = fill;
            look.bg_active = fill;
            look.fg = value(interaction::DISABLED_FG);
            look.fg_hover = value(interaction::DISABLED_FG);
            look.border = value(interaction::DISABLED_BORDER);
        }
        look
    }
}
