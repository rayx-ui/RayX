//! `Combobox`: RayX's cross-platform searchable combobox, built on `nsh/base`'s [`Combobox`].
//!
//! `nsh/base`'s `Combobox` owns the behavior: filtering by typing, single or multiple selection,
//! keyboard navigation (Up/Down/Home/End/PageUp/PageDown/Return/Escape/Tab), clearable values,
//! desktop popups and touch sheets, and combobox/listbox accessibility roles. This component gives
//! it RayX's look from RayX's theme roles only, so light/dark modes, accents, density, and custom
//! themes apply automatically on macOS, Windows, Linux, iOS, Android, and web.

use nsh_base::{ComboboxEvent as BaseComboboxEvent, SelectEntry, SelectGroup, SelectItem};
use nsh_core::element::Typography;
use nsh_core::prelude::*;

use crate::theme::roles::{
    BORDER_WIDTH_FOCUS_RING, BORDER_WIDTH_OUTLINE, INTENT_NEUTRAL_BG, INTENT_NEUTRAL_TEXT,
    INTENT_NEUTRAL_TEXT_MUTED, INTERACTION_DISABLED_BG, INTERACTION_DISABLED_BORDER,
    INTERACTION_DISABLED_FG, INTERACTION_FOCUS_RING, SURFACE_BORDER, SURFACE_BORDER_MUTED,
    SURFACE_PANEL1,
};
use crate::theme::{ComponentSize, value};

/// One selectable option in a [`Combobox`].
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct ComboboxItem {
    /// Stable option value compared against `value` and emitted in `onChange` and `onConfirm`.
    pub value: SharedString,
    /// Label text displayed for this option and matched while filtering.
    pub label: SharedString,
    /// Optional section header grouping adjacent options in the popup list.
    #[prop(skip_none)]
    pub group: Option<SharedString>,
    /// Disables this individual option so it cannot be chosen.
    #[prop(default)]
    pub disabled: bool,
}

impl ComboboxItem {
    /// Creates an enabled option with `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            group: None,
            disabled: false,
        }
    }

    /// Assigns this option to a named section header.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// Marks this option as disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The props of [`Combobox`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ComboboxProps {
    /// The selectable options, in order.
    #[prop(default)]
    pub items: Prop<Vec<ComboboxItem>>,
    /// Allows choosing multiple options: selecting toggles membership and keeps the popup open.
    #[prop(default)]
    pub multiple: Prop<bool>,
    /// Controlled selected option values; omit to let the combobox manage its own selection.
    pub value: Prop<Option<Vec<SharedString>>>,
    /// Initial selected option values when `value` is not set.
    #[prop(default)]
    pub default_value: Prop<Vec<SharedString>>,
    /// Placeholder text displayed when no option is selected and the input is empty.
    pub placeholder: Prop<Option<SharedString>>,
    /// The control's size: `small` for dense toolbars, `default` for standard forms, `large` for
    /// touch layouts.
    #[prop(default)]
    pub size: Prop<ComponentSize>,
    /// Displays a clear button while at least one option is selected.
    #[prop(default)]
    pub cleanable: Prop<bool>,
    /// Disables the combobox so it cannot open, focus, or change selection.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// Accessible name read by screen readers (e.g. `"Thumbnail size"`).
    pub accessibility_label: Prop<Option<SharedString>>,
    /// Test identifier attached to the combobox's text input element (`inputTestID` in TSX).
    #[prop(rename = "inputTestID")]
    pub input_test_id: Prop<Option<SharedString>>,
    /// Preferred side of the field where the desktop popup opens (default `"bottom"`).
    #[prop(default)]
    pub placement: Prop<Placement>,
    /// Cross-axis alignment of the desktop popup relative to the field (default `"start"`).
    #[prop(default = Align::Start)]
    pub align: Prop<Align>,
    /// Message shown inside the popup when no option matches the search query.
    pub empty_label: Prop<Option<SharedString>>,
}

/// The events of [`Combobox`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum ComboboxEvent {
    /// The selected values changed (a choice, a toggle, or clearing).
    #[args(value)]
    Change(Vec<SharedString>),
    /// The popup closed with the final selected values.
    #[args(value)]
    Confirm(Vec<SharedString>),
    /// The user edited the filter query text.
    #[args(query)]
    QueryChange(SharedString),
    /// The popup opened (`true`) or closed (`false`).
    #[args(open)]
    OpenChange(bool),
}

/// A cross-platform searchable combobox for choosing one or more options from a list.
///
/// Built on `nsh/base`'s `Combobox`, styled exclusively from RayX's theme roles so light/dark
/// modes, accents, density, and custom themes apply across desktop, mobile, and web.
///
/// # Rust
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_components::combobox::{ComboboxItem, combobox};
///
/// let selected = signal(vec![SharedString::new_static("md")]);
/// let _ = combobox()
///     .items(vec![
///         ComboboxItem::new("sm", "Small"),
///         ComboboxItem::new("md", "Medium"),
///         ComboboxItem::new("lg", "Large"),
///     ])
///     .value(move || Some(selected.get()))
///     .on_change(move |next: &Vec<SharedString>| selected.set(next.clone()));
/// ```
#[component]
pub fn Combobox(props: ComboboxProps, cx: Scope<ComboboxEvent>) -> impl IntoElement {
    let look = computed(move || Look::of(props.size.get(), props.disabled.get()));
    let select_items = computed(move || {
        props.items.with(|list| {
            list.iter()
                .map(|item| {
                    let mut entry = SelectItem::new(item.value.clone(), item.label.clone())
                        .disabled(item.disabled);
                    if let Some(group) = &item.group {
                        entry = entry.group(group.clone());
                    }
                    entry
                })
                .collect::<Vec<_>>()
        })
    });

    nsh_base::combobox()
        .items(select_items)
        .multiple(props.multiple)
        .value(props.value)
        .default_value(props.default_value)
        .placeholder(props.placeholder)
        .cleanable(props.cleanable)
        .disabled(props.disabled)
        .accessibility_label(props.accessibility_label)
        .input_test_id(props.input_test_id)
        .placement(props.placement)
        .align(props.align)
        .offset(px(4.0))
        .on_change(move |next: &Vec<SharedString>| cx.emit(ComboboxEvent::Change(next.clone())))
        .on_confirm(move |next: &Vec<SharedString>| cx.emit(ComboboxEvent::Confirm(next.clone())))
        .on_query_change(move |q: &SharedString| cx.emit(ComboboxEvent::QueryChange(q.clone())))
        .on_open_change(move |event: &BaseComboboxEvent| {
            if let BaseComboboxEvent::OpenChange(open, _) = event {
                cx.emit(ComboboxEvent::OpenChange(*open));
            }
        })
        .flex_row()
        .items_center()
        .h(move || px(look.with(|l| l.height)))
        .px(move || px(look.with(|l| l.inset)))
        .gap(move || px(look.with(|l| l.gap)))
        .rounded(move || px(look.with(|l| l.radius)))
        .bg(move || look.with(|l| l.bg))
        .border(move || px(look.with(|l| l.border_width)))
        .border_color(move || look.with(|l| l.border))
        .focus_within(|s| {
            s.outline(move || px(look.with(|l| l.ring_width)))
                .outline_color(move || look.with(|l| l.ring))
        })
        .input_style(move |s| {
            let l = look.get();
            s.flex_1()
                .min_w(px(0.0))
                .h_full()
                .text_color(l.fg)
                .placeholder_color(l.fg_muted)
                .typography(l.text)
        })
        .popup_style(move |s| {
            let l = look.get();
            s.p(px(4.0))
                .rounded(px(l.radius))
                .border(px(l.border_width))
                .border_color(l.popup_border)
                .bg(l.popup_bg)
                .shadow_md()
        })
        .popup_class("transition-all duration-120 ease-out")
        .popup_starting_class("opacity-0 -translate-y-1")
        .popup_ending_class("opacity-0")
        .backdrop_class("bg-black/40")
        .group_style(move |s| {
            let l = look.get();
            s.px(px(l.inset)).py(px(4.0))
        })
        .item_class(
            "group rounded-small focus:bg-intent-neutral-bg-hover selected:bg-intent-primary-bg-selected disabled:opacity-50",
        )
        .item_style(move |s| {
            let l = look.get();
            s.px(px(l.inset))
                .py(px(l.item_py))
                .rounded(px(l.inner_radius))
                .text_color(l.fg)
                .typography(l.text)
        })
        .clear_button(
            text("×")
                .text_color(move || look.with(|l| l.fg_muted))
                .typography(move || look.with(|l| l.text.clone())),
        )
        .trigger_button(
            view()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .pl(px(2.0))
                .child(
                    text("▾")
                        .text_color(move || {
                            look.with(|l| {
                                if props.disabled.get() {
                                    l.fg_disabled
                                } else {
                                    l.fg_muted
                                }
                            })
                        })
                        .typography(move || look.with(|l| l.text.clone())),
                ),
        )
        .empty(
            view().px(px(8.0)).py(px(6.0)).child(
                text(move || {
                    props
                        .empty_label
                        .get()
                        .unwrap_or_else(|| SharedString::new_static("No matches"))
                })
                .text_color(move || look.with(|l| l.fg_muted))
                .typography(move || look.with(|l| l.text.clone())),
            ),
        )
        .render_group(move |group: &SelectGroup| {
            text(group.title.clone())
                .number_of_lines(1u32)
                .text_color(move || look.with(|l| l.fg_muted))
                .typography(move || look.with(|l| l.text_group.clone()))
        })
        .render_item(move |entry: &SelectEntry| {
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    text(entry.title.clone())
                        .flex_1()
                        .number_of_lines(1u32),
                )
                .child(text("✓").app_class("opacity-0 group-selected:opacity-100"))
        })
}

#[derive(Debug, Clone, PartialEq)]
struct Look {
    bg: Hsla,
    border: Hsla,
    popup_bg: Hsla,
    popup_border: Hsla,
    fg: Hsla,
    fg_muted: Hsla,
    fg_disabled: Hsla,
    border_width: f32,
    ring: Hsla,
    ring_width: f32,
    height: f32,
    inset: f32,
    item_py: f32,
    gap: f32,
    radius: f32,
    inner_radius: f32,
    text: Typography,
    text_group: Typography,
}

impl Look {
    fn of(size: ComponentSize, disabled: bool) -> Self {
        let sizes = size.roles();
        let level = sizes.level(5);
        let bg = if disabled {
            value(INTERACTION_DISABLED_BG)
        } else {
            value(INTENT_NEUTRAL_BG)
        };
        let border = if disabled {
            value(INTERACTION_DISABLED_BORDER)
        } else {
            value(SURFACE_BORDER_MUTED)
        };
        let fg = if disabled {
            value(INTERACTION_DISABLED_FG)
        } else {
            value(INTENT_NEUTRAL_TEXT)
        };
        let item_py = match size {
            ComponentSize::Small => 4.0,
            ComponentSize::Default => 6.0,
            ComponentSize::Large => 8.0,
        };
        Self {
            bg,
            border,
            popup_bg: value(SURFACE_PANEL1),
            popup_border: value(SURFACE_BORDER),
            fg,
            fg_muted: value(INTENT_NEUTRAL_TEXT_MUTED),
            fg_disabled: value(INTERACTION_DISABLED_FG),
            border_width: value(BORDER_WIDTH_OUTLINE),
            ring: value(INTERACTION_FOCUS_RING),
            ring_width: value(BORDER_WIDTH_FOCUS_RING),
            height: value(level.height),
            inset: value(level.inset),
            item_py,
            gap: value(sizes.content).min(6.0),
            radius: value(sizes.radius),
            inner_radius: value(sizes.radius_inner),
            text: value(sizes.text.title1_regular),
            text_group: value(sizes.text.title1_bold),
        }
    }
}
