//! `HoverCard`: RayX's themed hover preview card, built on `nsh/base`'s
//! [`hover_card`](nsh_base::hover_card).
//!
//! `nsh/base`'s `HoverCard` owns pointer hover tracking, native open/close delay timers, touch
//! press disclosure, viewport-clamped anchored overlay positioning, Escape and outside-press
//! dismissal, and lazy child mounting. This module wraps it with RayX's elevated panel surface,
//! border, radius, inset padding, shadow, and subtle entrance/exit transitions — styled strictly
//! from RayX theme roles, with a `disabled` guard that closes any open card and suppresses hover
//! activation without disturbing the trigger's layout or events.

use std::sync::Arc;

pub use nsh_base::OpenChangeReason;
use nsh_core::BoxShadow;
use nsh_core::element::{Props as _, Style, StyleRefinement};
use nsh_core::prelude::*;

use crate::theme::roles::{
    BORDER_WIDTH_OUTLINE, RADIUS_DEFAULT, SHADOW_LG, SPACING_INSET3, SURFACE_BORDER_MUTED,
    SURFACE_PANEL1,
};
use crate::theme::value;

/// The props of [`HoverCard`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct HoverCardProps {
    /// Open (controlled): the app owns the open state and updates it in `onOpenChange`.
    /// Omit to let the hover card manage its own open state.
    pub open: Prop<Option<bool>>,
    /// Open initially while `open` is uncontrolled.
    #[prop(default)]
    pub default_open: Prop<bool>,
    /// Milliseconds the pointer rests on the trigger before the card opens (default `600`).
    /// Pass `0` to open immediately on hover.
    #[prop(default = 600.0)]
    pub open_delay: Prop<f64>,
    /// Milliseconds after the pointer leaves both the trigger and the card before closing
    /// (default `300`).
    #[prop(default = 300.0)]
    pub close_delay: Prop<f64>,
    /// Preferred side of the trigger the popup appears on (default `"bottom"`).
    #[prop(default)]
    pub placement: Prop<Placement>,
    /// Alignment of the popup relative to the trigger along that side (default `"center"`).
    #[prop(default)]
    pub align: Prop<Align>,
    /// Gap in points between the trigger and the popup (default `6`).
    #[prop(default = Points(6.0))]
    pub offset: Prop<Points>,
    /// Minimum margin in points kept from the viewport safe area edges.
    #[prop(default)]
    pub margin: Prop<Points>,
    /// Disables the hover card, immediately closing any open popup and suppressing hover
    /// activation while preserving the trigger's layout and events.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// Optional Tailwind utility classes applied to the outer trigger wrapper (for example
    /// `"h-full"` inside a flex row or tab strip).
    pub class: Prop<Option<SharedString>>,
    /// Optional Tailwind utility classes applied to the popup surface over RayX's default look.
    pub popup_class: Prop<Option<SharedString>>,
    /// Optional inline style merged on top of RayX's default popup surface look.
    #[prop(default)]
    pub popup_style: Prop<Style>,
    /// Element that anchors the card and opens it when hovered (or pressed on touch devices).
    pub trigger: Prop<Option<Slot>>,
    /// Card body content, mounted lazily only while the popup is open.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of [`HoverCard`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum HoverCardEvent {
    /// The hover card asked to open or close, with the reason it changed.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// The hover card popup is now shown.
    Open,
    /// The hover card popup has closed and finished any exit transition.
    Close,
    /// The popup was placed relative to its trigger, reporting the resolved side, frame, and
    /// available space.
    #[args(placement, bounds, available)]
    Position(Placement, Bounds<Points>, Size<Points>),
}

/// A themed hover preview card anchored to its `trigger`, built on `nsh/base`'s `HoverCard`.
///
/// Renders its floating popup with RayX's panel surface, muted border, outline width, default
/// radius, inset padding, large shadow, and subtle entrance/exit transitions, while allowing
/// callers to layer `popupClass` and `popupStyle` overrides on top.
#[component]
pub fn HoverCard(props: HoverCardProps, cx: Scope<HoverCardEvent>) -> impl IntoElement {
    let look = computed(HoverCardLook::of);
    let own = signal(props.default_open.get_untracked());

    // Reset uncontrolled state when disabled so re-enabling after the pointer left never
    // re-opens a stale card.
    effect(move || {
        if props.disabled.get() {
            own.set(false);
        }
    });

    let effective_open = move || {
        if props.disabled.get() {
            Some(false)
        } else {
            Some(props.open.get().unwrap_or_else(|| own.get()))
        }
    };

    let popup_style = computed(move || {
        let l = look.get();
        let base = Style::from(
            StyleRefinement::default()
                .p(px(l.inset))
                .rounded(px(l.radius))
                .bg(l.bg)
                .border(px(l.border_width))
                .border_color(l.border)
                .shadow(l.shadow.iter().copied())
                .transition_all()
                .duration(120.0_f32)
                .ease_out(),
        );
        base.then(&props.popup_style.get())
    });

    let mut base_props = nsh_base::HoverCardProps::initial();
    base_props.open = Prop::from(effective_open.into_reactive());
    base_props.open_delay = props.open_delay;
    base_props.close_delay = props.close_delay;
    base_props.placement = props.placement;
    base_props.align = props.align;
    base_props.offset = props.offset;
    base_props.margin = props.margin;
    base_props.popup_class = props.popup_class;
    base_props.popup_style = Prop::from(popup_style.into_reactive());
    base_props.popup_starting_class =
        Prop::new(Some(SharedString::new_static("opacity-0 -translate-y-1")));
    base_props.popup_ending_class = Prop::new(Some(SharedString::new_static("opacity-0")));

    nsh_base::HoverCard::element(base_props)
        .app_class(props.class)
        .on_event(move |e: &nsh_base::HoverCardEvent| match *e {
            nsh_base::HoverCardEvent::OpenChange(open, reason) => {
                if props.disabled.get_untracked() {
                    return;
                }
                cx.emit(HoverCardEvent::OpenChange(open, reason));
                if props.open.get_untracked().is_none() {
                    own.set(open);
                }
            }
            nsh_base::HoverCardEvent::Open => cx.emit(HoverCardEvent::Open),
            nsh_base::HoverCardEvent::Close => cx.emit(HoverCardEvent::Close),
            nsh_base::HoverCardEvent::Position(placement, bounds, available) => {
                cx.emit(HoverCardEvent::Position(placement, bounds, available));
            }
        })
        .fill("trigger", props.trigger.place(|t| t))
        .child(props.children)
}

/// Resolved RayX theme values for a [`HoverCard`] popup surface. Public only for crate tests.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct HoverCardLook {
    pub bg: Hsla,
    pub border: Hsla,
    pub border_width: f32,
    pub radius: f32,
    pub inset: f32,
    pub shadow: Arc<[BoxShadow]>,
}

impl HoverCardLook {
    /// Reads the hover card popup roles in the active theme context.
    pub fn of() -> Self {
        Self {
            bg: value(SURFACE_PANEL1),
            border: value(SURFACE_BORDER_MUTED),
            border_width: value(BORDER_WIDTH_OUTLINE),
            radius: value(RADIUS_DEFAULT),
            inset: value(SPACING_INSET3),
            shadow: value(SHADOW_LG),
        }
    }
}
