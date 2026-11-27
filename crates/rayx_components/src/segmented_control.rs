//! `SegmentedControl`: RayX's cross-platform segmented selector, built on `nsh/base`'s [`Tabs`]
//! and [`Tab`].
//!
//! `nsh/base`'s `Tabs` owns the behavior: roving keyboard focus (Left/Right arrows, Home/End),
//! tablist/tab accessibility roles, selected state, disabled state, and the native spring-animated
//! selection indicator (`View::follow`). This component gives it RayX's look from RayX's theme
//! roles only, so light/dark modes, accents, density, and custom themes apply automatically on
//! macOS, Windows, Linux, iOS, Android, and web.

use nsh_core::element::{Typography, each};
use nsh_core::geometry::Axis;
use nsh_core::prelude::*;

use crate::theme::roles::{
    BORDER_WIDTH_FOCUS_RING, BORDER_WIDTH_OUTLINE, INTENT_NEUTRAL_BG, INTENT_NEUTRAL_BG_STRONG,
    INTENT_NEUTRAL_TEXT, INTENT_NEUTRAL_TEXT_MUTED, INTERACTION_DISABLED_BG,
    INTERACTION_DISABLED_BORDER, INTERACTION_DISABLED_FG, INTERACTION_FOCUS_RING, SURFACE_BORDER,
    SURFACE_BORDER_MUTED, SURFACE_PANEL1,
};
use crate::theme::{ComponentSize, value};

/// One segment option in a [`SegmentedControl`].
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct SegmentedItem {
    /// Stable value identifier compared against `value` and emitted in `onChange`.
    pub value: SharedString,
    /// Label text displayed inside the segment.
    pub label: SharedString,
    /// Optional short count or badge shown beside the label.
    #[prop(skip_none)]
    pub badge: Option<SharedString>,
    /// Disables this individual segment so it cannot be selected.
    #[prop(default)]
    pub disabled: bool,
    /// Accessible name read by screen readers when different from `label`.
    #[prop(skip_none)]
    pub accessibility_label: Option<SharedString>,
}

impl SegmentedItem {
    /// Creates an enabled segment with `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            badge: None,
            disabled: false,
            accessibility_label: None,
        }
    }

    /// Sets a short badge or count displayed beside the label.
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// Marks this segment as disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Sets an explicit accessible name for screen readers.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
}

/// The props of [`SegmentedControl`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SegmentedControlProps {
    /// The segments to display, in order.
    #[prop(default)]
    pub items: Prop<Vec<SegmentedItem>>,
    /// The `value` of the currently selected segment.
    #[prop(default)]
    pub value: Prop<SharedString>,
    /// The control's size: `small` for dense toolbars, `default` for standard headers, `large`
    /// for touch layouts.
    #[prop(default)]
    pub size: Prop<ComponentSize>,
    /// Disables the entire control so no segment can be selected.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// Accessible name for the segment list (e.g. `"Filter by kind"`).
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// The events of [`SegmentedControl`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SegmentedControlEvent {
    /// The user selected another segment (by pointer, touch, or Left/Right/Home/End keys).
    #[args(value)]
    Change(SharedString),
}

/// A cross-platform segmented control for switching between mutually exclusive views or filters.
///
/// Built on `nsh/base`'s `Tabs` and `Tab`, with a spring-animated selection indicator, horizontal
/// scroll-safe flex layout (`flex_shrink_0`), and keyboard Left/Right/Home/End navigation.
///
/// # Rust
///
/// ```no_run
/// use nsh_core::prelude::*;
/// use rayx_components::segmented_control::{SegmentedItem, segmented_control};
///
/// let selected = signal(SharedString::new_static("all"));
/// let _ = segmented_control()
///     .items(vec![
///         SegmentedItem::new("all", "All"),
///         SegmentedItem::new("icons", "Icons"),
///     ])
///     .value(selected)
///     .on_change(move |next: &SharedString| selected.set(next.clone()));
/// ```
#[component]
pub fn SegmentedControl(
    props: SegmentedControlProps,
    cx: Scope<SegmentedControlEvent>,
) -> impl IntoElement {
    let look = computed(move || Look::of(props.size.get(), props.disabled.get()));

    nsh_base::tabs()
        .test_id("segmented-control")
        .orientation(Some(Axis::Horizontal))
        .accessibility_label(props.accessibility_label)
        .indicator_style(move |s| {
            let l = look.get();
            s.top(px(l.inner_inset))
                .bottom(px(l.inner_inset))
                .bg(l.indicator_bg)
                .rounded(px(l.inner_radius))
                .border(px(l.border_width))
                .border_color(l.indicator_border)
        })
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .h(move || px(look.with(|l| l.height)))
        .p(move || px(look.with(|l| l.inner_inset)))
        .gap(move || px(look.with(|l| l.item_gap)))
        .rounded(move || px(look.with(|l| l.radius)))
        .bg(move || look.with(|l| l.track_bg))
        .border(move || px(look.with(|l| l.border_width)))
        .border_color(move || look.with(|l| l.track_border))
        .child(each(
            props.items,
            |item| item.value.clone(),
            move |item, _| {
                let initial_value = item.with_untracked(|i| i.value.clone());
                let test_id = format!("segmented-item-{initial_value}");

                let is_selected = move || {
                    props
                        .value
                        .with(|current| item.with(|i| current.as_str() == i.value.as_str()))
                };
                let is_disabled = move || props.disabled.get() || item.with(|i| i.disabled);

                nsh_base::tab()
                    .test_id(&test_id)
                    .selected(is_selected)
                    .disabled(is_disabled)
                    .accessibility_label(move || item.with(|i| i.accessibility_label.clone()))
                    .on_press(move || {
                        if props.disabled.get_untracked() || item.with_untracked(|i| i.disabled) {
                            return;
                        }
                        let target = item.with_untracked(|i| i.value.clone());
                        if props.value.with_untracked(|current| current != &target) {
                            cx.emit(SegmentedControlEvent::Change(target));
                        }
                    })
                    .flex_row()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .h(move || px(look.with(|l| l.inner_height)))
                    .px(move || px(look.with(|l| l.inset)))
                    .gap(move || px(look.with(|l| l.content_gap)))
                    .rounded(move || px(look.with(|l| l.inner_radius)))
                    .text_color(move || {
                        look.with(|l| {
                            if is_disabled() {
                                l.fg_disabled
                            } else if is_selected() {
                                l.fg_selected
                            } else {
                                l.fg
                            }
                        })
                    })
                    .typography(move || {
                        look.with(|l| {
                            if is_selected() {
                                l.text_selected.clone()
                            } else {
                                l.text.clone()
                            }
                        })
                    })
                    .transition_colors()
                    .duration(120.0_f32)
                    .ease_out()
                    .hover(|s| {
                        s.text_color(move || {
                            look.with(|l| {
                                if is_disabled() {
                                    l.fg_disabled
                                } else {
                                    l.fg_selected
                                }
                            })
                        })
                    })
                    .focus_visible(|s| {
                        s.outline(move || px(look.with(|l| l.ring_width)))
                            .outline_color(move || look.with(|l| l.ring))
                    })
                    .child(text(move || item.with(|i| i.label.clone())).number_of_lines(1u32))
                    .child(when(
                        move || item.with(|i| i.badge.is_some()),
                        move || {
                            text(move || item.with(|i| i.badge.clone().unwrap_or_default()))
                                .number_of_lines(1u32)
                                .text_color(move || {
                                    look.with(|l| {
                                        if is_disabled() {
                                            l.fg_disabled
                                        } else {
                                            l.fg_muted
                                        }
                                    })
                                })
                        },
                    ))
            },
        ))
}

#[derive(Debug, Clone, PartialEq)]
struct Look {
    track_bg: Hsla,
    track_border: Hsla,
    indicator_bg: Hsla,
    indicator_border: Hsla,
    fg: Hsla,
    fg_selected: Hsla,
    fg_muted: Hsla,
    fg_disabled: Hsla,
    border_width: f32,
    ring: Hsla,
    ring_width: f32,
    height: f32,
    inner_height: f32,
    inner_inset: f32,
    inset: f32,
    content_gap: f32,
    item_gap: f32,
    radius: f32,
    inner_radius: f32,
    text: Typography,
    text_selected: Typography,
}

impl Look {
    fn of(size: ComponentSize, disabled: bool) -> Self {
        let sizes = size.roles();
        let level = sizes.level(5);
        let track_bg = if disabled {
            value(INTERACTION_DISABLED_BG)
        } else {
            value(INTENT_NEUTRAL_BG)
        };
        let track_border = if disabled {
            value(INTERACTION_DISABLED_BORDER)
        } else {
            value(SURFACE_BORDER_MUTED)
        };
        let indicator_bg = if disabled {
            value(INTENT_NEUTRAL_BG_STRONG)
        } else {
            value(SURFACE_PANEL1)
        };
        let indicator_border = if disabled {
            value(INTERACTION_DISABLED_BORDER)
        } else {
            value(SURFACE_BORDER)
        };
        Self {
            track_bg,
            track_border,
            indicator_bg,
            indicator_border,
            fg: value(INTENT_NEUTRAL_TEXT_MUTED),
            fg_selected: value(INTENT_NEUTRAL_TEXT),
            fg_muted: value(INTENT_NEUTRAL_TEXT_MUTED),
            fg_disabled: value(INTERACTION_DISABLED_FG),
            border_width: value(BORDER_WIDTH_OUTLINE),
            ring: value(INTERACTION_FOCUS_RING),
            ring_width: value(BORDER_WIDTH_FOCUS_RING),
            height: value(level.height),
            inner_height: value(level.inner_height),
            inner_inset: value(level.inner_inset),
            inset: value(level.inset),
            content_gap: value(sizes.content),
            item_gap: value(sizes.items).min(4.0),
            radius: value(sizes.radius),
            inner_radius: value(sizes.radius_inner),
            text: value(sizes.text.title1_regular),
            text_selected: value(sizes.text.title1_bold),
        }
    }
}
