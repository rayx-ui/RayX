//! RayX's `Dialog` in a Rust view, headless: theme role parity (`SURFACE_SCRIM`,
//! `SURFACE_PANEL1`, `SURFACE_BORDER_MUTED`, `SURFACE_TEXT`, `SURFACE_TEXT_MUTED`, spacing and
//! radius roles), controlled `open` visibility, `role="alertdialog"`, header/body/footer rendering,
//! and Escape / backdrop cancel behavior.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::sync::Arc;

use nsh_core::element::{Root, Token, Tokens};
use nsh_core::prelude::*;
use nsh_core::{EventKind, EventPayload};
use nsh_engine::App;
use rayx_components::button::{ButtonVariant, button};
use rayx_components::dialog::{DialogEvent, DialogLook, OpenChangeReason, dialog};
use support::{Running, rgba};

thread_local! {
    static LOOK: RefCell<Option<(DialogLook, Arc<Tokens>)>> = const { RefCell::new(None) };
}

#[component]
fn DialogLookProbe() -> impl IntoElement {
    effect(move || {
        let look = DialogLook::of();
        let toks = tokens();
        LOOK.with(|slot| *slot.borrow_mut() = Some((look, toks)));
    });
    text("dialog-look-ready")
}

#[test]
fn dialog_look_reads_exact_rayx_theme_roles() {
    LOOK.with(|slot| *slot.borrow_mut() = None);
    let mut app = Running::start(App::Rust(Root::of::<DialogLookProbe>()));
    assert!(app.wait(|s| s.text_node("dialog-look-ready").is_some()));

    let (look, tokens) = LOOK.with(|slot| slot.borrow().clone()).expect("look");
    let color = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Color(c)) => rgba(c),
        other => panic!("{role}: {other:?}"),
    };
    let number = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Number(n)) => n,
        other => panic!("{role}: {other:?}"),
    };

    assert_eq!(rgba(look.scrim), color("surface-scrim"));
    assert_eq!(rgba(look.surface), color("surface-panel1"));
    assert_eq!(rgba(look.border), color("surface-border-muted"));
    assert_eq!(look.border_width, number("border-width-outline"));
    assert_eq!(look.radius, number("radius-default"));
    assert_eq!(look.inset, number("spacing-inset4"));
    assert_eq!(look.header_gap, number("spacing-gap2"));
    assert_eq!(look.body_gap, number("spacing-gap4"));
    assert_eq!(look.footer_gap, number("spacing-gap3"));
    assert_eq!(rgba(look.text), color("surface-text"));
    assert_eq!(rgba(look.text_muted), color("surface-text-muted"));
}

#[component]
fn DialogHarness() -> impl IntoElement {
    let open = signal(false);
    let close_on_backdrop = signal(true);
    let close_on_escape = signal(true);
    let cancels = signal(0_u32);
    let last_reason = signal(String::from("none"));

    view()
        .items_start()
        .gap_2()
        .child(
            button()
                .text("Open Dialog")
                .on_press(move || open.set(true)),
        )
        .child(button().text("Lock Backdrop").on_press(move || {
            close_on_backdrop.set(false);
            close_on_escape.set(false);
        }))
        .child(
            dialog()
                .open(open)
                .role(Some(AccessibilityRole::AlertDialog))
                .title("Delete item?")
                .description("This moves the item to the trash.")
                .close_on_escape(close_on_escape)
                .close_on_backdrop_press(close_on_backdrop)
                .on_cancel(move || cancels.update(|n| *n += 1))
                .on_open_change(move |e: &DialogEvent| {
                    if let DialogEvent::OpenChange(next, reason) = *e {
                        last_reason.set(match reason {
                            OpenChangeReason::BackdropPress => "backdrop".into(),
                            OpenChangeReason::Cancel => "escape".into(),
                            other => format!("{other:?}"),
                        });
                        open.set(next);
                    }
                })
                .footer(
                    view()
                        .flex_row()
                        .gap_2()
                        .child(button().text("Cancel").on_press(move || open.set(false)))
                        .child(
                            button()
                                .text("Confirm Delete")
                                .variant(ButtonVariant::Danger)
                                .on_press(move || {
                                    last_reason.set("confirmed".into());
                                    open.set(false);
                                }),
                        ),
                )
                .child(text("Body details")),
        )
        .child(text(move || {
            format!(
                "open={} cancels={} reason={}",
                open.get(),
                cancels.get(),
                last_reason.get()
            )
        }))
}

#[test]
fn dialog_opens_closes_and_handles_escape_and_backdrop_cancel() {
    let mut app = Running::start(App::Rust(Root::of::<DialogHarness>()));
    assert!(
        app.wait(|s| {
            s.pressable("Open Dialog").is_some()
                && s.text_node("open=false cancels=0 reason=none").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    // Closed dialog does not mount its title/body/footer.
    assert!(app.screen.text_node("Delete item?").is_none());

    // Open the dialog.
    app.press("Open Dialog");
    assert!(
        app.wait(|s| {
            s.text_node("Delete item?").is_some()
                && s.text_node("This moves the item to the trash.").is_some()
                && s.text_node("Body details").is_some()
                && s.pressable("Cancel").is_some()
                && s.pressable("Confirm Delete").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Pressing the modal's Escape/RequestClose event triggers Cancel + OpenChange(false, Cancel).
    let modal = *app
        .screen
        .events
        .iter()
        .find(|(_, mask)| mask.contains(EventKind::RequestClose))
        .expect("modal listens to RequestClose")
        .0;
    app.engine
        .control()
        .dispatch_event(modal, EventKind::RequestClose, EventPayload::None);
    assert!(
        app.wait(|s| {
            s.text_node("open=false cancels=1 reason=escape").is_some()
                && s.text_node("Delete item?").is_none()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Re-open and press the backdrop (sibling before card inside modal container).
    app.press("Open Dialog");
    assert!(app.wait(|s| s.text_node("Delete item?").is_some()));

    let backdrop = app
        .screen
        .events
        .iter()
        .find(|(id, mask)| {
            mask.contains(EventKind::Press)
                && app
                    .screen
                    .styles
                    .get(id)
                    .is_some_and(|st| st.background_color.0 == support::color("surface-scrim"))
        })
        .map(|(id, _)| *id)
        .expect("scrim backdrop pressable");
    app.press_node(backdrop);
    assert!(
        app.wait(|s| {
            s.text_node("open=false cancels=2 reason=backdrop")
                .is_some()
                && s.text_node("Delete item?").is_none()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Disable Escape and backdrop closing, re-open, and verify Cancel fires without closing.
    app.press("Lock Backdrop");
    app.press("Open Dialog");
    assert!(app.wait(|s| s.text_node("Delete item?").is_some()));

    let modal = *app
        .screen
        .events
        .iter()
        .find(|(_, mask)| mask.contains(EventKind::RequestClose))
        .expect("modal listens to RequestClose")
        .0;
    app.engine
        .control()
        .dispatch_event(modal, EventKind::RequestClose, EventPayload::None);
    assert!(
        app.wait(|s| s.text_node("open=true cancels=3 reason=backdrop").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(
        app.screen.text_node("Delete item?").is_some(),
        "dialog stays open when close_on_escape is false"
    );

    // Explicit confirm button closes the dialog.
    app.press("Confirm Delete");
    assert!(
        app.wait(|s| {
            s.text_node("open=false cancels=3 reason=confirmed")
                .is_some()
                && s.text_node("Delete item?").is_none()
        }),
        "{:?}",
        app.screen.shown()
    );
}
