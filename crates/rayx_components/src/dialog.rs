//! `Dialog`: RayX's themed modal dialog, built on `nsh/base`'s [`dialog`](nsh_base::dialog).
//!
//! `nsh/base`'s `dialog` (the core `Modal` element) owns modal presentation, focus trapping,
//! Escape/back handling, and `dialog` / `alertdialog` accessibility semantics. This module wraps
//! it with RayX's scrim backdrop, centered card surface, heading/description typography, optional
//! body children, and right-aligned footer slot — styled strictly from RayX theme roles.

use std::sync::Arc;

pub use nsh_base::OpenChangeReason;
use nsh_core::BoxShadow;
use nsh_core::element::Typography;
use nsh_core::prelude::*;

use crate::theme::roles::{
    BORDER_WIDTH_OUTLINE, RADIUS_DEFAULT, SHADOW_LG, SPACING_GAP2, SPACING_GAP3, SPACING_GAP4,
    SPACING_INSET4, SURFACE_BORDER_MUTED, SURFACE_PANEL1, SURFACE_SCRIM, SURFACE_TEXT,
    SURFACE_TEXT_MUTED, TEXT_DEFAULT_REGULAR, TEXT_DEFAULT_TITLE1_BOLD,
};
use crate::theme::value;

/// The props of [`Dialog`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct DialogProps {
    /// Whether the dialog is currently presented (controlled by the app).
    #[prop(default)]
    pub open: Prop<bool>,
    /// Optional title heading displayed at the top of the dialog card. Also serves as the default
    /// accessible name when `accessibilityLabel` is omitted.
    pub title: Prop<Option<SharedString>>,
    /// Optional explanatory text displayed below `title`. Also serves as the default accessible
    /// description when `accessibilityDescription` is omitted.
    pub description: Prop<Option<SharedString>>,
    /// Accessibility role of the modal (`"dialog"` by default, or `"alertdialog"` for confirmation
    /// and destructive prompts).
    pub role: Prop<Option<AccessibilityRole>>,
    /// Presentation animation (`"fade"` by default, `"slide"`, or `"none"`).
    #[prop(default = ModalAnimation::Fade)]
    pub animation: Prop<ModalAnimation>,
    /// Whether Escape / Android back requests closing the dialog (default `true`).
    #[prop(default = true)]
    pub close_on_escape: Prop<bool>,
    /// Whether pressing the scrim backdrop requests closing the dialog (default `true`).
    #[prop(default = true)]
    pub close_on_backdrop_press: Prop<bool>,
    /// Optional footer slot (typically action buttons) rendered right-aligned at the bottom of
    /// the dialog card.
    pub footer: Prop<Option<Slot>>,
    /// Explicit accessible name for screen readers (falls back to `title`).
    pub accessibility_label: Prop<Option<SharedString>>,
    /// Explicit accessible description for screen readers (falls back to `description`).
    pub accessibility_description: Prop<Option<SharedString>>,
    /// Optional body content rendered inside the dialog card while open.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of [`Dialog`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum DialogEvent {
    /// The user requested opening or closing the dialog (for example via Escape or backdrop
    /// press), with the reason. Update `open` in response to follow it.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// The dialog finished appearing.
    Open,
    /// The dialog closed.
    Close,
    /// The user tried to dismiss the dialog (Escape, Android back, or backdrop press), whether or
    /// not `closeOnEscape` / `closeOnBackdropPress` allow closing.
    Cancel,
}

/// A centered modal dialog over a dimmed scrim backdrop, built on `nsh/base`'s `Dialog`.
///
/// Supports both regular dialogs (`role="dialog"`, the default) and alert confirmation dialogs
/// (`role="alertdialog"`), with optional `title`, `description`, body `children`, and right-aligned
/// `footer` actions, styled strictly with RayX theme roles.
#[component]
pub fn Dialog(props: DialogProps, cx: Scope<DialogEvent>) -> impl IntoElement {
    let look = computed(DialogLook::of);

    // Zero-size, non-blocking anchor view so the component always mounts a stable host node
    // whether open or closed, without affecting sibling flex layout or gaps.
    view()
        .absolute()
        .size_0()
        .pointer_events(PointerEvents::BoxNone)
        .child(when(
            move || props.open.get(),
            move || {
                let header = when(
                    move || {
                        props
                            .title
                            .with(|t| t.as_ref().is_some_and(|s| !s.is_empty()))
                            || props
                                .description
                                .with(|d| d.as_ref().is_some_and(|s| !s.is_empty()))
                    },
                    move || {
                        view()
                            .gap(move || px(look.with(|l| l.header_gap)))
                            .child(when(
                                move || {
                                    props
                                        .title
                                        .with(|t| t.as_ref().is_some_and(|s| !s.is_empty()))
                                },
                                move || {
                                    text(move || props.title.get().unwrap_or_default())
                                        .typography(move || look.with(|l| l.title_text.clone()))
                                        .text_color(move || look.with(|l| l.text))
                                },
                            ))
                            .child(when(
                                move || {
                                    props
                                        .description
                                        .with(|d| d.as_ref().is_some_and(|s| !s.is_empty()))
                                },
                                move || {
                                    text(move || props.description.get().unwrap_or_default())
                                        .typography(move || {
                                            look.with(|l| l.description_text.clone())
                                        })
                                        .text_color(move || look.with(|l| l.text_muted))
                                },
                            ))
                    },
                );

                let footer = when(
                    move || props.footer.with(Option::is_some),
                    move || {
                        view()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap(move || px(look.with(|l| l.footer_gap)))
                            .child(props.footer.place(|f| f))
                    },
                );

                let backdrop = view()
                    .test_id("rayx-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .focusable(false)
                    .bg(move || look.with(|l| l.scrim))
                    .on_press(move || {
                        cx.emit(DialogEvent::Cancel);
                        if props.close_on_backdrop_press.get_untracked() {
                            cx.emit(DialogEvent::OpenChange(
                                false,
                                OpenChangeReason::BackdropPress,
                            ));
                        }
                    });

                let card = view()
                    .test_id("rayx-dialog-surface")
                    .w_full()
                    .max_w(px(420.0))
                    .p(move || px(look.with(|l| l.inset)))
                    .gap(move || px(look.with(|l| l.body_gap)))
                    .rounded(move || px(look.with(|l| l.radius)))
                    .bg(move || look.with(|l| l.surface))
                    .border(move || px(look.with(|l| l.border_width)))
                    .border_color(move || look.with(|l| l.border))
                    .adaptive_shadow(move || {
                        look.with(|l| AdaptiveShadow::from(l.shadow.iter().copied()))
                    })
                    .child(header)
                    .child(props.children)
                    .child(footer);

                nsh_base::dialog()
                    .role(move || props.role.get().unwrap_or(AccessibilityRole::Dialog))
                    .optional_label(move || {
                        props
                            .accessibility_label
                            .get()
                            .or_else(|| props.title.get())
                    })
                    .optional_accessibility_description(move || {
                        props
                            .accessibility_description
                            .get()
                            .or_else(|| props.description.get())
                    })
                    .animation(props.animation)
                    .close_on_escape(props.close_on_escape)
                    .close_on_backdrop_press(props.close_on_backdrop_press)
                    .on_open(move || cx.emit(DialogEvent::Open))
                    .on_close(move || cx.emit(DialogEvent::Close))
                    .on_cancel(move || {
                        cx.emit(DialogEvent::Cancel);
                        if props.close_on_escape.get_untracked() {
                            cx.emit(DialogEvent::OpenChange(false, OpenChangeReason::Cancel));
                        }
                    })
                    .child(
                        view()
                            .flex_1()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .p(move || px(look.with(|l| l.inset)))
                            .child(backdrop)
                            .child(card),
                    )
            },
        ))
}

/// Resolved RayX theme values for a [`Dialog`]. Public only for crate tests.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct DialogLook {
    pub scrim: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
    pub border_width: f32,
    pub radius: f32,
    pub inset: f32,
    pub header_gap: f32,
    pub body_gap: f32,
    pub footer_gap: f32,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub title_text: Typography,
    pub description_text: Typography,
    pub shadow: Arc<[BoxShadow]>,
}

impl DialogLook {
    /// Reads the dialog roles in the active theme context.
    pub fn of() -> Self {
        Self {
            scrim: value(SURFACE_SCRIM),
            surface: value(SURFACE_PANEL1),
            border: value(SURFACE_BORDER_MUTED),
            border_width: value(BORDER_WIDTH_OUTLINE),
            radius: value(RADIUS_DEFAULT),
            inset: value(SPACING_INSET4),
            header_gap: value(SPACING_GAP2),
            body_gap: value(SPACING_GAP4),
            footer_gap: value(SPACING_GAP3),
            text: value(SURFACE_TEXT),
            text_muted: value(SURFACE_TEXT_MUTED),
            title_text: value(TEXT_DEFAULT_TITLE1_BOLD),
            description_text: value(TEXT_DEFAULT_REGULAR),
            shadow: value(SHADOW_LG),
        }
    }
}
