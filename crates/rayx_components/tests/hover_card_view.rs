//! RayX's `HoverCard` in Rust views, headless: theme role parity in light and dark themes
//! (`SURFACE_PANEL1`, `SURFACE_BORDER_MUTED`, `BORDER_WIDTH_OUTLINE`, `RADIUS_DEFAULT`,
//! `SPACING_INSET3`, `SHADOW_LG`), controlled and uncontrolled open/close, `open_delay(0.0)`
//! immediate opening, `disabled` suppression and immediate closing without disturbing trigger
//! presses, and lazy child mounting.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use nsh_core::ApproxTextEngine;
use nsh_core::element::{Root, Token, Tokens, set_modifier};
use nsh_core::platform::{Environment, Idiom, Os};
use nsh_core::prelude::*;
use nsh_core::{EventKind, EventPayload, MouseEvent, NodeId};
use nsh_engine::{App, EngineConfig, LocalEngine};
use rayx_components::button::button;
use rayx_components::hover_card::{HoverCardEvent, HoverCardLook, OpenChangeReason, hover_card};
use support::{ChannelSink, Running, Screen, rgba, ui_sets};

fn start_desktop(app: App) -> Running {
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = LocalEngine::start(
        EngineConfig {
            ui_sets: ui_sets(),
            environment: Environment {
                os: Os::Windows,
                idiom: Idiom::Desktop,
                ..Environment::default()
            },
            ..EngineConfig::new(app)
        },
        ApproxTextEngine,
        ChannelSink(tx),
    )
    .unwrap();
    Running {
        engine,
        rx,
        screen: Screen::default(),
    }
}

thread_local! {
    static LOOK: RefCell<Option<(HoverCardLook, Arc<Tokens>)>> = const { RefCell::new(None) };
}

#[component]
fn HoverCardLookProbe() -> impl IntoElement {
    let mode = signal("light");
    effect(move || {
        let look = HoverCardLook::of();
        let toks = tokens();
        LOOK.with(|slot| *slot.borrow_mut() = Some((look, toks)));
    });
    view()
        .child(button().text("Switch Dark").on_press(move || {
            set_modifier("colorMode", "dark");
            mode.set("dark");
        }))
        .child(text(move || format!("hover-card-look-{}", mode.get())))
}

fn assert_look_matches_tokens(look: &HoverCardLook, tokens: &Tokens) {
    let color = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Color(c)) => rgba(c),
        other => panic!("{role}: {other:?}"),
    };
    let number = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Number(n)) => n,
        other => panic!("{role}: {other:?}"),
    };
    let shadow = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Shadow(s)) => s,
        other => panic!("{role}: {other:?}"),
    };

    assert_eq!(rgba(look.bg), color("surface-panel1"));
    assert_eq!(rgba(look.border), color("surface-border-muted"));
    assert_eq!(look.border_width, number("border-width-outline"));
    assert_eq!(look.radius, number("radius-default"));
    assert_eq!(look.inset, number("spacing-inset3"));
    assert_eq!(look.shadow, shadow("shadow-lg"));
    assert!(!look.shadow.is_empty());
}

#[test]
fn hover_card_look_reads_exact_rayx_theme_roles_in_light_and_dark_themes() {
    LOOK.with(|slot| *slot.borrow_mut() = None);
    let mut app = Running::start(App::Rust(Root::of::<HoverCardLookProbe>()));
    assert!(app.wait(|s| {
        s.text_node("hover-card-look-light").is_some() && s.pressable("Switch Dark").is_some()
    }));

    let (light_look, light_tokens) = LOOK.with(|slot| slot.borrow().clone()).expect("light look");
    assert_look_matches_tokens(&light_look, &light_tokens);

    app.press("Switch Dark");
    assert!(app.wait(|s| s.text_node("hover-card-look-dark").is_some()));

    let (dark_look, dark_tokens) = LOOK.with(|slot| slot.borrow().clone()).expect("dark look");
    assert_look_matches_tokens(&dark_look, &dark_tokens);
    assert_ne!(
        rgba(light_look.bg),
        rgba(dark_look.bg),
        "dark theme resolves a distinct surface-panel1 color"
    );
}

fn find_mouse_enter_ancestor(screen: &Screen, label: &str) -> NodeId {
    let mut current = screen
        .text_node(label)
        .unwrap_or_else(|| panic!("no text node `{label}`: {:?}", screen.shown()));
    loop {
        if screen
            .events
            .get(&current)
            .is_some_and(|mask| mask.contains(EventKind::MouseEnter))
        {
            return current;
        }
        current = *screen
            .parents
            .get(&current)
            .unwrap_or_else(|| panic!("no MouseEnter ancestor for `{label}`"));
    }
}

static LAZY_UNCONTROLLED_MOUNTS: AtomicU32 = AtomicU32::new(0);
static LAZY_CONTROLLED_MOUNTS: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, PartialEq, Props)]
struct LazyBodyProps {
    #[prop(default)]
    pub label: Prop<SharedString>,
    #[prop(default)]
    pub controlled: Prop<bool>,
}

#[component]
fn LazyBody(props: LazyBodyProps, cx: Scope<NoEvent>) -> impl IntoElement {
    let _ = cx;
    if props.controlled.get_untracked() {
        LAZY_CONTROLLED_MOUNTS.fetch_add(1, Ordering::Relaxed);
    } else {
        LAZY_UNCONTROLLED_MOUNTS.fetch_add(1, Ordering::Relaxed);
    }
    text(props.label)
}

#[component]
fn HoverCardHarness() -> impl IntoElement {
    let controlled_open = signal(false);
    let controlled_disabled = signal(false);
    let uncontrolled_disabled = signal(false);
    let trigger_presses = signal(0_u32);
    let open_changes = signal(0_u32);
    let last_change = signal(String::from("none"));

    view()
        .items_start()
        .gap_2()
        .child(
            hover_card()
                .open_delay(0.0)
                .close_delay(0.0)
                .placement(Placement::Bottom)
                .align(Align::Start)
                .offset(Points(4.0))
                .disabled(uncontrolled_disabled)
                .class("h-full")
                .on_open_change(move |e: &HoverCardEvent| {
                    if let HoverCardEvent::OpenChange(open, reason) = *e {
                        open_changes.update(|n| *n += 1);
                        let reason_label = match reason {
                            OpenChangeReason::Hover => "hover",
                            OpenChangeReason::Cancel => "cancel",
                            OpenChangeReason::PressOutside => "outside",
                            _ => "other",
                        };
                        last_change.set(format!("{open}:{reason_label}"));
                    }
                })
                .trigger(
                    view()
                        .on_press(move || trigger_presses.update(|n| *n += 1))
                        .child(text("Hover Trigger")),
                )
                .child(lazy_body().label("Uncontrolled Card Body")),
        )
        .child(
            hover_card()
                .open(move || Some(controlled_open.get()))
                .disabled(controlled_disabled)
                .trigger(view().child(text("Controlled Trigger")))
                .child(lazy_body().label("Controlled Card Body").controlled(true)),
        )
        .child(button().text("Disable Uncontrolled").on_press(move || {
            uncontrolled_disabled.set(true);
        }))
        .child(button().text("Enable Uncontrolled").on_press(move || {
            uncontrolled_disabled.set(false);
        }))
        .child(button().text("Open Controlled").on_press(move || {
            controlled_open.set(true);
        }))
        .child(button().text("Close Controlled").on_press(move || {
            controlled_open.set(false);
        }))
        .child(button().text("Disable Controlled").on_press(move || {
            controlled_disabled.set(true);
        }))
        .child(text(move || {
            format!(
                "presses={} changes={} last={}",
                trigger_presses.get(),
                open_changes.get(),
                last_change.get(),
            )
        }))
}

#[test]
fn hover_card_supports_controlled_uncontrolled_zero_delay_lazy_children_and_disabled() {
    LAZY_UNCONTROLLED_MOUNTS.store(0, Ordering::Relaxed);
    LAZY_CONTROLLED_MOUNTS.store(0, Ordering::Relaxed);

    let mut app = start_desktop(App::Rust(Root::of::<HoverCardHarness>()));
    assert!(
        app.wait(|s| {
            s.text_node("Hover Trigger").is_some()
                && s.text_node("presses=0 changes=0 last=none").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    // Lazy children stay unmounted while closed.
    assert!(app.screen.text_node("Uncontrolled Card Body").is_none());
    assert!(app.screen.text_node("Controlled Card Body").is_none());
    assert_eq!(LAZY_UNCONTROLLED_MOUNTS.load(Ordering::Relaxed), 0);
    assert_eq!(LAZY_CONTROLLED_MOUNTS.load(Ordering::Relaxed), 0);

    // Controlled open mounts lazy children; controlled close unmounts them.
    app.press("Open Controlled");
    assert!(
        app.wait(|s| s.text_node("Controlled Card Body").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert_eq!(LAZY_CONTROLLED_MOUNTS.load(Ordering::Relaxed), 1);

    // Disabling a controlled open card closes it immediately.
    app.press("Disable Controlled");
    assert!(
        app.wait(|s| s.text_node("Controlled Card Body").is_none()),
        "{:?}",
        app.screen.shown()
    );
    app.press("Close Controlled");

    // Uncontrolled hover with open_delay(0.0) opens immediately on MouseEnter.
    let trigger = find_mouse_enter_ancestor(&app.screen, "Hover Trigger");
    app.engine.control().dispatch_event(
        trigger,
        EventKind::MouseEnter,
        EventPayload::Mouse(MouseEvent::default()),
    );
    assert!(
        app.wait(|s| {
            s.text_node("Uncontrolled Card Body").is_some()
                && s.text_node("presses=0 changes=1 last=true:hover").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    assert_eq!(LAZY_UNCONTROLLED_MOUNTS.load(Ordering::Relaxed), 1);

    // Disabling closes the open uncontrolled card immediately.
    app.press("Disable Uncontrolled");
    assert!(
        app.wait(|s| s.text_node("Uncontrolled Card Body").is_none()),
        "{:?}",
        app.screen.shown()
    );

    // Trigger press events continue to work while disabled.
    app.press("Hover Trigger");
    assert!(
        app.wait(|s| s.text_node("presses=1 changes=1 last=true:hover").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Leaving and re-entering while disabled does not open or emit OpenChange.
    app.engine.control().dispatch_event(
        trigger,
        EventKind::MouseLeave,
        EventPayload::Mouse(MouseEvent::default()),
    );
    app.engine.control().dispatch_event(
        trigger,
        EventKind::MouseEnter,
        EventPayload::Mouse(MouseEvent::default()),
    );
    assert!(
        app.wait(|s| {
            s.text_node("Uncontrolled Card Body").is_none()
                && s.text_node("presses=1 changes=1 last=true:hover").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    assert_eq!(LAZY_UNCONTROLLED_MOUNTS.load(Ordering::Relaxed), 1);

    // Leaving while disabled and then re-enabling does not ghost-reopen the card.
    app.engine.control().dispatch_event(
        trigger,
        EventKind::MouseLeave,
        EventPayload::Mouse(MouseEvent::default()),
    );
    app.press("Enable Uncontrolled");
    assert!(
        app.wait(|s| {
            s.text_node("Uncontrolled Card Body").is_none()
                && s.text_node("presses=1 changes=1 last=true:hover").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Hovering after re-enabling opens again, and MouseLeave closes immediately with close_delay(0.0).
    app.engine.control().dispatch_event(
        trigger,
        EventKind::MouseEnter,
        EventPayload::Mouse(MouseEvent::default()),
    );
    assert!(
        app.wait(|s| {
            s.text_node("Uncontrolled Card Body").is_some()
                && s.text_node("presses=1 changes=2 last=true:hover").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    assert_eq!(LAZY_UNCONTROLLED_MOUNTS.load(Ordering::Relaxed), 2);

    app.engine.control().dispatch_event(
        trigger,
        EventKind::MouseLeave,
        EventPayload::Mouse(MouseEvent::default()),
    );
    assert!(
        app.wait(|s| {
            s.text_node("Uncontrolled Card Body").is_none()
                && s.text_node("presses=1 changes=3 last=false:hover")
                    .is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
}
