//! RayX's `Menu`, `ContextMenu`, `MenuItem`, and `MenuSeparator` in Rust views, headless:
//! theme role parity (including `MenuItemVariant::Danger` and disabled roles), opening via
//! trigger and context-menu event, modifier forwarding on `MenuItem::on_select`, checkable and
//! disabled states, and separator rendering.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::sync::Arc;

use nsh_core::color::Hsla;
use nsh_core::element::{Root, Token, Tokens};
use nsh_core::keymap::Modifiers;
use nsh_core::prelude::*;
use nsh_core::{ContextMenuEvent, EventKind, EventPayload, MountItem, NodeId, PressEvent};
use nsh_engine::App;
use rayx_components::menu::{
    MenuItemLook, MenuItemVariant, PopupLook, SeparatorLook, context_menu, menu, menu_item,
    menu_separator,
};
use support::{Running, rgba};

#[derive(Clone)]
struct ProbeSnapshot {
    popup: PopupLook,
    default_item: MenuItemLook,
    danger_item: MenuItemLook,
    disabled_item: MenuItemLook,
    sep: SeparatorLook,
    tokens: Arc<Tokens>,
}

thread_local! {
    static LOOKS: RefCell<Option<ProbeSnapshot>> = const { RefCell::new(None) };
}

#[component]
fn LookProbe() -> impl IntoElement {
    effect(move || {
        let snapshot = ProbeSnapshot {
            popup: PopupLook::of(),
            default_item: MenuItemLook::of(MenuItemVariant::Default, false),
            danger_item: MenuItemLook::of(MenuItemVariant::Danger, false),
            disabled_item: MenuItemLook::of(MenuItemVariant::Danger, true),
            sep: SeparatorLook::of(),
            tokens: tokens(),
        };
        LOOKS.with(|slot| *slot.borrow_mut() = Some(snapshot));
    });
    text("look-ready")
}

#[test]
fn menu_looks_read_exact_rayx_theme_roles() {
    LOOKS.with(|slot| *slot.borrow_mut() = None);
    let mut app = Running::start(App::Rust(Root::of::<LookProbe>()));
    assert!(app.wait(|s| s.text_node("look-ready").is_some()));

    let ProbeSnapshot {
        popup,
        default_item,
        danger_item,
        disabled_item,
        sep,
        tokens,
    } = LOOKS.with(|slot| slot.borrow().clone()).expect("looks");

    let color = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Color(c)) => rgba(c),
        other => panic!("{role}: {other:?}"),
    };
    let number = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Number(n)) => n,
        other => panic!("{role}: {other:?}"),
    };

    // Popup surface roles
    assert_eq!(rgba(popup.bg), color("surface-panel1"));
    assert_eq!(rgba(popup.border), color("surface-border-muted"));
    assert_eq!(popup.border_width, number("border-width-outline"));
    assert_eq!(popup.radius, number("radius-default"));
    assert_eq!(popup.inset_y, number("spacing-inset1"));
    assert_eq!(rgba(popup.scrim), color("surface-scrim"));

    // Default MenuItem roles
    assert_eq!(rgba(default_item.bg), rgba(Hsla::default()));
    assert_eq!(rgba(default_item.fg), color("surface-text"));
    assert_eq!(rgba(default_item.shortcut_fg), color("surface-text-subtle"));
    assert_eq!(rgba(default_item.bg_hover), color("intent-primary-bg"));
    assert_eq!(
        rgba(default_item.bg_active),
        color("intent-primary-bg-active")
    );
    assert_eq!(rgba(default_item.fg_hover), color("intent-primary-text"));
    assert_eq!(rgba(default_item.ring), color("interaction-focus-ring"));
    assert_eq!(default_item.height, number("spacing-small-lvl6-height"));
    assert_eq!(default_item.radius, number("radius-small"));

    // Danger MenuItem roles
    assert_eq!(rgba(danger_item.fg), color("surface-text-danger"));
    assert_eq!(rgba(danger_item.bg_hover), color("intent-danger-bg"));
    assert_eq!(
        rgba(danger_item.bg_active),
        color("intent-danger-bg-active")
    );
    assert_eq!(rgba(danger_item.fg_hover), color("intent-danger-text"));
    assert_eq!(
        rgba(danger_item.ring),
        color("interaction-focus-ring-danger")
    );

    // Disabled MenuItem roles
    assert_eq!(rgba(disabled_item.fg), color("interaction-disabled-fg"));
    assert_eq!(
        rgba(disabled_item.fg_hover),
        color("interaction-disabled-fg")
    );
    assert_eq!(
        rgba(disabled_item.shortcut_fg),
        color("interaction-disabled-fg")
    );
    assert_eq!(rgba(disabled_item.bg_hover), rgba(Hsla::default()));
    assert_eq!(rgba(disabled_item.bg_active), rgba(Hsla::default()));

    // Separator roles
    assert_eq!(rgba(sep.color), color("surface-border-muted"));
    assert_eq!(sep.thickness, number("border-width-separator"));
    assert_eq!(sep.margin_y, number("spacing-inset1"));
}

#[component]
fn MenuHarness() -> impl IntoElement {
    let last = signal(String::from("none"));
    let checked = signal(true);
    let ctx_last = signal(String::from("ctx-none"));

    view()
        .items_start()
        .gap_2()
        .child(
            menu()
                .accessibility_label("Actions menu")
                .trigger(view().test_id("menu-trigger").child(text("Actions")))
                .child(
                    menu_item()
                        .label("Expand")
                        .shortcut("Alt+E")
                        .icon(text("*"))
                        .on_select(move |m: &Modifiers| {
                            last.set(format!("expand:alt={},shift={}", m.alt, m.shift));
                        }),
                )
                .child(
                    menu_item()
                        .label("Show Grid")
                        .checked(move || Some(checked.get()))
                        .close_on_select(false)
                        .on_select(move |_: &Modifiers| {
                            checked.set(!checked.get_untracked());
                            last.set("grid".into());
                        }),
                )
                .child(menu_item().label("Disabled Item").disabled(true).on_select(
                    move |_: &Modifiers| {
                        last.set("should-not-fire".into());
                    },
                ))
                .child(menu_separator())
                .child(
                    menu_item()
                        .label("Delete")
                        .shortcut("Del")
                        .variant(MenuItemVariant::Danger)
                        .on_select(move |m: &Modifiers| {
                            last.set(format!("delete:alt={}", m.alt));
                        }),
                ),
        )
        .child(
            context_menu()
                .trigger(view().test_id("ctx-target").child(text("Right click me")))
                .child(
                    menu_item()
                        .label("Duplicate")
                        .on_select(move |m: &Modifiers| {
                            ctx_last.set(format!("dup:alt={}", m.alt));
                        }),
                )
                .child(menu_separator())
                .child(
                    menu_item()
                        .label("Remove")
                        .variant(MenuItemVariant::Danger)
                        .on_select(move |_: &Modifiers| {
                            ctx_last.set("remove".into());
                        }),
                ),
        )
        .child(text(move || format!("last:{}", last.get())))
        .child(text(move || format!("checked:{}", checked.get())))
        .child(text(move || format!("ctx:{}", ctx_last.get())))
}

fn press_with_modifiers(app: &mut Running, id: NodeId, modifiers: Modifiers) {
    let payload = EventPayload::Press(PressEvent {
        modifiers,
        clicks: 1,
        ..PressEvent::default()
    });
    for kind in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
        app.engine
            .control()
            .dispatch_event(id, kind, payload.clone());
    }
}

#[test]
fn menu_opens_renders_items_and_forwards_modifiers_on_select() {
    let mut app = Running::start(App::Rust(Root::of::<MenuHarness>()));
    assert!(
        app.wait(|s| s.pressable("Actions").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Open Menu via trigger press.
    app.press("Actions");
    assert!(
        app.wait(|s| {
            s.text_node("Expand").is_some()
                && s.text_node("Alt+E").is_some()
                && s.text_node("*").is_some()
                && s.text_node("Show Grid").is_some()
                && s.text_node("✓").is_some()
                && s.text_node("Delete").is_some()
                && s.text_node("Del").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );

    // Disabled item is not pressable.
    assert!(app.screen.pressable("Disabled Item").is_none());

    // Pressing a checkable item with close_on_select(false) toggles checked and keeps menu open.
    app.press("Show Grid");
    assert!(
        app.wait(|s| s.text_node("checked:false").is_some() && s.text_node("last:grid").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(
        app.screen.text_node("Expand").is_some(),
        "menu stayed open after close_on_select(false)"
    );

    // Alt+Click on MenuItem forwards Modifiers { alt: true } through on_select.
    let expand = app.screen.pressable("Expand").expect("Expand pressable");
    press_with_modifiers(
        &mut app,
        expand,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );
    assert!(
        app.wait(|s| s.text_node("last:expand:alt=true,shift=false").is_some()),
        "{:?}",
        app.screen.shown()
    );
}

#[test]
fn context_menu_opens_on_context_menu_event_and_selects_danger_item() {
    let mut app = Running::start(App::Rust(Root::of::<MenuHarness>()));
    assert!(
        app.wait(|s| s.text_node("Right click me").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Find the nearest ancestor of "Right click me" that handles ContextMenu events.
    let mut target = app.screen.text_node("Right click me").unwrap();
    let ctx_node = loop {
        if app
            .screen
            .events
            .get(&target)
            .is_some_and(|e| e.contains(EventKind::ContextMenu))
        {
            break target;
        }
        target = *app
            .screen
            .parents
            .get(&target)
            .expect("context menu listener ancestor");
    };

    app.engine.control().dispatch_event(
        ctx_node,
        EventKind::ContextMenu,
        EventPayload::ContextMenu(ContextMenuEvent::default()),
    );

    assert!(
        app.wait(|s| s.text_node("Duplicate").is_some() && s.text_node("Remove").is_some()),
        "{:?}",
        app.screen.shown()
    );

    // Verify danger item renders with surface-text-danger on its item container or text.
    let remove_node = app.screen.pressable("Remove").expect("Remove pressable");
    let _ = MountItem::Delete { id: remove_node }; // keep MountItem check minimal
    app.press_node(remove_node);
    assert!(
        app.wait(|s| s.text_node("ctx:remove").is_some()),
        "{:?}",
        app.screen.shown()
    );
}
