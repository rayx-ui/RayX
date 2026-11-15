//! `Button`: RayX's button (Figma `buttons/Button`), on nsh-base's `Button`.
//!
//! nsh-base's `Button` owns the behavior: presses, keyboard activation, focus, the disabled
//! state and the role screen readers hear. This file gives it RayX's look, from the theme's
//! tokens: the variant's role colors (`theme.colors.roles.<role>`), the size's level 5
//! (`platform.density.size.<size>.lvl5`), its spacing, radius and `title1Bold` text style.

use nsh_core::prelude::*;

use crate::theme::{ComponentSize, RayXTheme, RoleColors, TextStyle, rayx, weight};

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
            &rayx(),
            props.variant.get(),
            props.size.get(),
            props.outlined.get(),
            props.disabled.get(),
        )
    });
    let at_rest = rayx();
    let at_rest = at_rest.platform.density.size.of(props.size.get_untracked());
    // A weight and an outline width are no bindings in NSH: read once, from the theme the
    // button is made in (the same for every size and density in the export).
    let font_weight = weight(at_rest.text.title1_bold.font_weight);
    let ring_width = rayx().platform.density.borders.focus_ring_width;

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
        .text_size(move || px(look.with(|l| l.text.font_size)))
        .line_height(move || px(look.with(|l| l.text.line_height)))
        .letter_spacing(move || px(look.with(|l| l.text.letter_spacing)))
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
    pub(crate) text: TextStyle,
}

impl Look {
    /// The tokens Figma's `buttons/Button` binds, for these props.
    pub(crate) fn of(
        theme: &RayXTheme,
        variant: ButtonVariant,
        size: ComponentSize,
        outlined: bool,
        disabled: bool,
    ) -> Self {
        let colors = &theme.theme.colors;
        let sizes = theme.platform.density.size.of(size);
        let none = AdaptiveColor::TRANSPARENT;
        let neutral = &colors.roles.neutral;
        let role = |r: &RoleColors| (r.bg, r.bg_hover, r.bg_active, r.text, r.text);
        let (bg, bg_hover, bg_active, fg, fg_hover) = match variant {
            ButtonVariant::Neutral => (
                neutral.bg,
                neutral.bg_hover,
                neutral.bg_active,
                neutral.text,
                neutral.text,
            ),
            ButtonVariant::Primary => role(&colors.roles.primary),
            ButtonVariant::Danger => role(&colors.roles.danger),
            ButtonVariant::Success => role(&colors.roles.success),
            ButtonVariant::Ghost => (
                none,
                neutral.bg_hover,
                neutral.bg_active,
                neutral.text,
                neutral.text,
            ),
            ButtonVariant::Link => (
                none,
                none,
                none,
                colors.surfaces.link,
                colors.surfaces.link_hover,
            ),
        };
        let interaction = &colors.interaction;
        let ring = match variant {
            ButtonVariant::Danger => interaction.focus_ring_danger,
            ButtonVariant::Success => interaction.focus_ring_success,
            _ => interaction.focus_ring,
        };
        let outlined = outlined && variant != ButtonVariant::Link;
        let border_width = if outlined {
            theme.platform.density.borders.outline_width
        } else {
            0.0
        };
        let level = &sizes.lvl5;
        let mut look = Look {
            bg,
            bg_hover,
            bg_active,
            fg,
            fg_hover,
            border: colors.surfaces.border,
            border_width,
            ring,
            height: level.height,
            inset: level.inset,
            gap: sizes.spacing.content,
            radius: sizes.borders.radius,
            adornment: level.adornment_size,
            text: sizes.text.title1_bold.clone(),
        };
        if disabled {
            let fill = match variant {
                ButtonVariant::Ghost | ButtonVariant::Link => none,
                _ => interaction.disabled_bg,
            };
            look.bg = fill;
            look.bg_hover = fill;
            look.bg_active = fill;
            look.fg = interaction.disabled_fg;
            look.fg_hover = interaction.disabled_fg;
            look.border = interaction.disabled_border;
        }
        look
    }
}
