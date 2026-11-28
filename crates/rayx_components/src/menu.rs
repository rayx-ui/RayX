//! Menus: [`Menu`], [`ContextMenu`], [`MenuItem`], and [`MenuSeparator`], built on `nsh/base`'s
//! menu primitives.
//!
//! `nsh/base` owns the behavior: anchored popup positioning, viewport clamping and scrolling,
//! desktop right-click / touch long-press / keyboard Shift+F10 and accessibility triggers, roving
//! arrow-key and typeahead focus, focus restoration, Escape/outside dismissal, and checkable
//! accessibility roles. This module gives them RayX's look from RayX's theme roles only, so
//! light/dark modes, accents, density, and custom themes apply automatically.

use std::sync::Arc;

pub use nsh_base::OpenChangeReason;
use nsh_core::BoxShadow;
use nsh_core::element::Typography;
use nsh_core::keymap::Modifiers;
use nsh_core::prelude::*;

use crate::theme::roles::{
    BORDER_WIDTH_FOCUS_RING, BORDER_WIDTH_OUTLINE, BORDER_WIDTH_SEPARATOR, INTERACTION_DISABLED_FG,
    INTERACTION_FOCUS_RING, INTERACTION_FOCUS_RING_DANGER, RADIUS_DEFAULT, SHADOW_MD, SPACING_GAP2,
    SPACING_GAP3, SPACING_INSET1, SPACING_INSET3, SURFACE_BORDER_MUTED, SURFACE_PANEL1,
    SURFACE_SCRIM, SURFACE_TEXT, SURFACE_TEXT_DANGER, SURFACE_TEXT_SUBTLE,
};
use crate::theme::{ComponentSize, IntentRoles, value};

/// The props of [`Menu`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct MenuProps {
    /// Open (controlled): the app owns the open state and updates it in `onOpenChange`.
    /// Without it the menu manages its own open state.
    pub open: Prop<Option<bool>>,
    /// Open initially while `open` is uncontrolled.
    #[prop(default)]
    pub default_open: Prop<bool>,
    /// Side of the trigger the popup appears on (default `"bottom"`).
    #[prop(default)]
    pub placement: Prop<Placement>,
    /// Alignment of the popup relative to the trigger (default `"start"`).
    #[prop(default = Align::Start)]
    pub align: Prop<Align>,
    /// Gap in points between the trigger and the popup (default `4`).
    #[prop(default = Points(4.0))]
    pub offset: Prop<Points>,
    /// Minimum margin in points kept from the viewport safe area edges (default `8`).
    #[prop(default = Points(8.0))]
    pub margin: Prop<Points>,
    /// Element that opens the menu when pressed.
    pub trigger: Prop<Option<Slot>>,
    /// Accessible name for the menu container when its trigger content is not enough.
    pub accessibility_label: Prop<Option<SharedString>>,
    /// Menu contents (`MenuItem`, `MenuSeparator`, and custom elements), created when opened.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of [`Menu`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum MenuEvent {
    /// The menu opened or closed, with the reason it changed.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// The menu is now shown.
    Open,
    /// The menu has closed.
    Close,
    /// The user dismissed the menu without choosing an item (Escape, outside press, or backdrop).
    Cancel,
}

/// A themed dropdown menu opened by pressing its `trigger`, built on `nsh/base`'s `Menu`.
///
/// Renders a floating popup styled with RayX's surface, border, radius, spacing, and shadow roles
/// (or a bottom sheet over `surface-scrim` on compact touch screens).
#[component]
pub fn Menu(props: MenuProps, cx: Scope<MenuEvent>) -> impl IntoElement {
    let look = computed(PopupLook::of);

    let menu_el = nsh_base::menu()
        .open(props.open)
        .default_open(props.default_open)
        .placement(props.placement)
        .align(props.align)
        .offset(props.offset)
        .margin(props.margin)
        .popup_style(move |s| {
            let l = look.get();
            s.min_w(px(180.0))
                .py(px(l.inset_y))
                .rounded(px(l.radius))
                .bg(l.bg)
                .border(px(l.border_width))
                .border_color(l.border)
                .shadow(l.shadow.iter().copied())
                .transition_all()
                .duration(120.0_f32)
                .ease_out()
        })
        .popup_starting_style(|s| s.opacity(0.0_f32).translate_y(px(-4.0)))
        .popup_ending_style(|s| s.opacity(0.0_f32))
        .backdrop_style(move |s| s.bg(look.with(|l| l.scrim)))
        .on_open_change(move |e: &nsh_base::MenuEvent| {
            if let nsh_base::MenuEvent::OpenChange(open, reason) = *e {
                cx.emit(MenuEvent::OpenChange(open, reason));
            }
        })
        .on_open(move || cx.emit(MenuEvent::Open))
        .on_close(move || cx.emit(MenuEvent::Close))
        .on_cancel(move || cx.emit(MenuEvent::Cancel))
        .trigger(props.trigger.place(|t| t))
        .child(props.children);

    view()
        .optional_label(props.accessibility_label)
        .child(menu_el)
}

/// The props of [`ContextMenu`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ContextMenuProps {
    /// Open (controlled): the app owns the open state and updates it in `onOpenChange`.
    /// Without it the context menu manages its own open state.
    pub open: Prop<Option<bool>>,
    /// Disables opening the context menu via secondary click, long press, or keyboard trigger.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// Gap in points between the pointer location and the popup (default `0`).
    #[prop(default)]
    pub offset: Prop<Points>,
    /// Minimum margin in points kept from the viewport safe area edges (default `8`).
    #[prop(default = Points(8.0))]
    pub margin: Prop<Points>,
    /// Target element that opens the context menu on secondary click, touch hold, Shift+F10, or
    /// assistive-technology context menu action.
    pub trigger: Prop<Option<Slot>>,
    /// Menu contents (`MenuItem`, `MenuSeparator`, and custom elements), created when opened.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of [`ContextMenu`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum ContextMenuEvent {
    /// The context menu opened or closed, with the reason it changed.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// The context menu is now shown.
    Open,
    /// The context menu has closed.
    Close,
    /// The user dismissed the context menu without choosing an item.
    Cancel,
}

/// A themed context menu opened on secondary click, long press, or keyboard menu trigger, built on
/// `nsh/base`'s `ContextMenu`.
///
/// Shares the exact same RayX popup surface styling as [`Menu`].
#[component]
pub fn ContextMenu(props: ContextMenuProps, cx: Scope<ContextMenuEvent>) -> impl IntoElement {
    let look = computed(PopupLook::of);

    nsh_base::context_menu()
        .open(props.open)
        .disabled(props.disabled)
        .offset(props.offset)
        .margin(props.margin)
        .popup_style(move |s| {
            let l = look.get();
            s.min_w(px(180.0))
                .py(px(l.inset_y))
                .rounded(px(l.radius))
                .bg(l.bg)
                .border(px(l.border_width))
                .border_color(l.border)
                .shadow(l.shadow.iter().copied())
                .transition_all()
                .duration(120.0_f32)
                .ease_out()
        })
        .popup_starting_style(|s| s.opacity(0.0_f32).translate_y(px(-4.0)))
        .popup_ending_style(|s| s.opacity(0.0_f32))
        .backdrop_style(move |s| s.bg(look.with(|l| l.scrim)))
        .on_open_change(move |e: &nsh_base::ContextMenuEvents| {
            if let nsh_base::ContextMenuEvents::OpenChange(open, reason) = *e {
                cx.emit(ContextMenuEvent::OpenChange(open, reason));
            }
        })
        .on_open(move || cx.emit(ContextMenuEvent::Open))
        .on_close(move || cx.emit(ContextMenuEvent::Close))
        .on_cancel(move || cx.emit(ContextMenuEvent::Cancel))
        .trigger(props.trigger.place(|t| t))
        .child(props.children)
}

/// Visual emphasis of a [`MenuItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PropEnum)]
pub enum MenuItemVariant {
    /// A standard menu action (the default).
    #[default]
    Default,
    /// A destructive action (such as deleting an item), styled with danger theme roles.
    Danger,
}

impl MenuItemVariant {
    /// Every menu item variant.
    pub const ALL: [MenuItemVariant; 2] = [MenuItemVariant::Default, MenuItemVariant::Danger];
}

/// The props of [`MenuItem`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct MenuItemProps {
    /// Primary text label shown in the row. When empty, `children` are rendered instead.
    #[prop(default)]
    pub label: Prop<SharedString>,
    /// Optional keyboard shortcut hint displayed at the trailing edge of the row.
    pub shortcut: Prop<Option<SharedString>>,
    /// Optional leading icon rendered at the row's adornment size in the item's text color.
    pub icon: Prop<Option<Slot>>,
    /// Visual emphasis: `"default"` or `"danger"`.
    #[prop(default)]
    pub variant: Prop<MenuItemVariant>,
    /// When `Some`, marks the item as checkable (`menuitemcheckbox`, or `role`) and shows a
    /// checkmark when `Some(true)`.
    pub checked: Prop<Option<bool>>,
    /// Disables the item so it cannot be chosen and renders with disabled theme roles.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// Whether choosing this item closes the enclosing menu (default `true`).
    #[prop(default = true)]
    pub close_on_select: Prop<bool>,
    /// Optional accessibility role override (for example `"menuitemradio"`).
    pub role: Prop<Option<AccessibilityRole>>,
    /// Explicit accessible name read by screen readers when different from `label`.
    pub accessibility_label: Prop<Option<SharedString>>,
    /// Secondary accessible description read by screen readers.
    pub accessibility_description: Prop<Option<SharedString>>,
    /// Optional custom content rendered when `label` is empty.
    pub children: Prop<Children>,
}

/// The events of [`MenuItem`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum MenuItemEvent {
    /// The item was selected, carrying the keyboard modifiers active at selection time
    /// (such as `alt` for Alt+Click).
    #[args(modifiers)]
    Select(Modifiers),
}

/// One selectable action inside a [`Menu`] or [`ContextMenu`], built on `nsh/base`'s `MenuItem`.
///
/// Styled strictly through RayX theme roles, supporting optional icons, right-aligned shortcut
/// labels, checkable state, disabled state, `danger` emphasis, and modifier forwarding on
/// `onSelect`.
#[component]
pub fn MenuItem(props: MenuItemProps, cx: Scope<MenuItemEvent>) -> impl IntoElement {
    let look = computed(move || MenuItemLook::of(props.variant.get(), props.disabled.get()));

    let check_mark = when(
        move || props.checked.get().is_some(),
        move || {
            view()
                .w(move || px(look.with(|l| l.adornment)))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(
                    text(move || {
                        if props.checked.get() == Some(true) {
                            "✓"
                        } else {
                            ""
                        }
                    })
                    .number_of_lines(1u32),
                )
        },
    );

    let leading_icon = when(
        move || props.icon.get().is_some(),
        move || {
            view()
                .size(move || px(look.with(|l| l.adornment)))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(props.icon.place(|s| s))
        },
    );

    let main_content = when(
        move || props.label.with(|l| !l.is_empty()),
        move || text(props.label).flex_1().number_of_lines(1u32),
    )
    .otherwise(move || view().flex_1().child(props.children));

    let trailing_shortcut = when(
        move || {
            props
                .shortcut
                .with(|s| s.as_ref().is_some_and(|v| !v.is_empty()))
        },
        move || {
            text(move || props.shortcut.get().unwrap_or_default())
                .flex_shrink_0()
                .number_of_lines(1u32)
                .typography(move || look.with(|l| l.shortcut_text.clone()))
                .text_color(move || look.with(|l| l.shortcut_fg))
                .group_hover(|s| s.text_color(move || look.with(|l| l.fg_hover)))
                .group_focus_within(|s| s.text_color(move || look.with(|l| l.fg_hover)))
                .group_active(|s| s.text_color(move || look.with(|l| l.fg_hover)))
        },
    );

    nsh_base::menu_item()
        .disabled(props.disabled)
        .checked(props.checked)
        .role(props.role)
        .close_on_select(props.close_on_select)
        .accessibility_label(props.accessibility_label)
        .accessibility_description(props.accessibility_description)
        .on_select(move |modifiers: &Modifiers| {
            if !props.disabled.get_untracked() {
                cx.emit(MenuItemEvent::Select(*modifiers));
            }
        })
        .group()
        .flex_row()
        .items_center()
        .h(move || px(look.with(|l| l.height)))
        .mx(move || px(look.with(|l| l.outer_inset)))
        .px(move || px(look.with(|l| l.inset_x)))
        .gap(move || px(look.with(|l| l.gap)))
        .rounded(move || px(look.with(|l| l.radius)))
        .bg(move || look.with(|l| l.bg))
        .text_color(move || look.with(|l| l.fg))
        .typography(move || look.with(|l| l.text.clone()))
        .transition_colors()
        .duration(100.0_f32)
        .ease_out()
        .hover(|s| {
            s.bg(move || look.with(|l| l.bg_hover))
                .text_color(move || look.with(|l| l.fg_hover))
        })
        .focus(|s| {
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
        .child(check_mark)
        .child(leading_icon)
        .child(main_content)
        .child(trailing_shortcut)
}

/// The props of [`MenuSeparator`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct MenuSeparatorProps {}

/// A horizontal divider separating groups of items in a [`Menu`] or [`ContextMenu`], built on
/// `nsh/base`'s `MenuSeparator`.
#[component]
pub fn MenuSeparator(_props: MenuSeparatorProps, cx: Scope<NoEvent>) -> impl IntoElement {
    let _ = cx;
    let look = computed(SeparatorLook::of);

    nsh_base::menu_separator()
        .h(move || px(look.with(|l| l.thickness)))
        .my(move || px(look.with(|l| l.margin_y)))
        .bg(move || look.with(|l| l.color))
}

/// Resolved RayX theme values for a menu popup surface. Public only for crate tests.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct PopupLook {
    pub bg: Hsla,
    pub border: Hsla,
    pub border_width: f32,
    pub radius: f32,
    pub inset_y: f32,
    pub scrim: Hsla,
    pub shadow: Arc<[BoxShadow]>,
}

impl PopupLook {
    /// Reads the menu popup roles in the active theme context.
    pub fn of() -> Self {
        Self {
            bg: value(SURFACE_PANEL1),
            border: value(SURFACE_BORDER_MUTED),
            border_width: value(BORDER_WIDTH_OUTLINE),
            radius: value(RADIUS_DEFAULT),
            inset_y: value(SPACING_INSET1),
            scrim: value(SURFACE_SCRIM),
            shadow: value(SHADOW_MD),
        }
    }
}

/// Resolved RayX theme values for a [`MenuItem`]. Public only for crate tests.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItemLook {
    pub bg: Hsla,
    pub bg_hover: Hsla,
    pub bg_active: Hsla,
    pub fg: Hsla,
    pub fg_hover: Hsla,
    pub shortcut_fg: Hsla,
    pub ring: Hsla,
    pub ring_width: f32,
    pub height: f32,
    pub outer_inset: f32,
    pub inset_x: f32,
    pub gap: f32,
    pub radius: f32,
    pub adornment: f32,
    pub text: Typography,
    pub shortcut_text: Typography,
}

impl MenuItemLook {
    /// Reads the menu item roles for `variant` and `disabled` in the active theme context.
    pub fn of(variant: MenuItemVariant, disabled: bool) -> Self {
        let sizes = ComponentSize::Small.roles();
        let row = sizes.level(6);
        let none = Hsla::default();

        let (fg, bg_hover, bg_active, fg_hover, shortcut_fg, ring) = match variant {
            MenuItemVariant::Default => (
                value(SURFACE_TEXT),
                value(IntentRoles::PRIMARY.bg),
                value(IntentRoles::PRIMARY.bg_active),
                value(IntentRoles::PRIMARY.text),
                value(SURFACE_TEXT_SUBTLE),
                value(INTERACTION_FOCUS_RING),
            ),
            MenuItemVariant::Danger => (
                value(SURFACE_TEXT_DANGER),
                value(IntentRoles::DANGER.bg),
                value(IntentRoles::DANGER.bg_active),
                value(IntentRoles::DANGER.text),
                value(SURFACE_TEXT_DANGER),
                value(INTERACTION_FOCUS_RING_DANGER),
            ),
        };

        let mut look = Self {
            bg: none,
            bg_hover,
            bg_active,
            fg,
            fg_hover,
            shortcut_fg,
            ring,
            ring_width: value(BORDER_WIDTH_FOCUS_RING),
            height: value(row.height),
            outer_inset: value(SPACING_INSET1),
            inset_x: value(SPACING_INSET3),
            gap: value(SPACING_GAP3).min(value(SPACING_GAP2) + value(SPACING_INSET1)),
            radius: value(sizes.radius),
            adornment: value(row.adornment_size),
            text: value(sizes.text.regular),
            shortcut_text: value(sizes.text.subtitle1_regular),
        };

        if disabled {
            let disabled_fg = value(INTERACTION_DISABLED_FG);
            look.bg = none;
            look.bg_hover = none;
            look.bg_active = none;
            look.fg = disabled_fg;
            look.fg_hover = disabled_fg;
            look.shortcut_fg = disabled_fg;
        }

        look
    }
}

/// Resolved RayX theme values for a [`MenuSeparator`]. Public only for crate tests.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct SeparatorLook {
    pub color: Hsla,
    pub thickness: f32,
    pub margin_y: f32,
}

impl SeparatorLook {
    /// Reads the separator roles in the active theme context.
    pub fn of() -> Self {
        Self {
            color: value(SURFACE_BORDER_MUTED),
            thickness: value(BORDER_WIDTH_SEPARATOR),
            margin_y: value(SPACING_INSET1),
        }
    }
}
