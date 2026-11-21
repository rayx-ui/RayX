//! RayX's `Slot` in design mode, headless: in an app a design tool started (NSH's design host
//! connected, so `design_mode()` holds), an empty slot draws a box to drop into, in RayX's roles,
//! with its name in it; a slot holding content draws nothing of its own; a slot that empties
//! draws the box. Every test of this group runs in design mode: the process connects as a design
//! host once, to a tool that listens and never answers, as NSH's own test of design mode does.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::sync::OnceLock;

use nsh_core::color::rgba;
use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_core::runtime::design::{self, DesignHooks};
use nsh_core::{BorderStyle, Color};
use nsh_engine::App;
use rayx_components::button::button;
use rayx_components::slot::slot;
use support::{Running, Screen};

/// Connects this process to a design tool, once: from then on `design_mode()` holds.
fn design_host() {
    static TOOL: OnceLock<std::net::TcpListener> = OnceLock::new();
    let tool = TOOL.get_or_init(|| std::net::TcpListener::bind("127.0.0.1:0").unwrap());
    let port = tool.local_addr().unwrap().port();
    design::connect(
        format!("tcp:127.0.0.1:{port}"),
        DesignHooks {
            input: Box::new(|_| {}),
            closed: Box::new(|| {}),
        },
    );
    assert!(design_mode());
}

#[component]
fn Slots() -> impl IntoElement {
    let shown = signal(true);
    view()
        .items_start()
        .gap(px(10.0))
        .child(slot().name("footer"))
        .child(slot().name("filled").child(text("Content")))
        .child(
            slot()
                .name("toggled")
                .child(when(move || shown.get(), || text("Comes and goes"))),
        )
        .child(
            button()
                .text("Toggle")
                .on_press(move || shown.update(|s| *s = !*s)),
        )
}

fn start() -> Running {
    design_host();
    let mut app = Running::start(App::Rust(Root::of::<Slots>()));
    assert!(
        app.wait(|s| {
            s.text_frame("footer").is_some()
                && s.text_frame("Content").is_some()
                && s.pressable("Toggle").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    app
}

fn color(role: &str) -> Color {
    Color::from(rgba(support::color(role)))
}

/// Whether the slot around `label` draws the empty box: its roles' fill, dashed edge, corners
/// and least size.
fn draws_the_box(screen: &Screen, label: &str) -> Result<(), String> {
    let host = screen.around(label).ok_or("no label")?;
    let style = screen.styles.get(&host).ok_or("not styled")?;
    let (width, height) = *screen.frames.get(&host).ok_or("not laid out")?;
    let size = support::number("spacing-default-lvl5-height");
    let checks = [
        (
            style.background_color == color("intent-primary-bg-translucent"),
            "fill",
        ),
        (
            style.borders.widths.top.0 == support::number("border-width-outline"),
            "edge width",
        ),
        (
            style.borders.colors[0] == color("intent-primary-accent"),
            "edge color",
        ),
        (style.borders.style == BorderStyle::Dashed, "dashed"),
        (
            style.border_radius.0 == support::number("radius-default"),
            "corners",
        ),
        (width >= size && height >= size, "least size"),
    ];
    match checks.iter().find(|(ok, _)| !ok) {
        None => Ok(()),
        Some((_, what)) => Err(format!("{what}: {style:?} {width}x{height}")),
    }
}

#[test]
fn an_empty_slot_draws_a_box_in_roles_with_its_name() {
    let app = start();
    let s = &app.screen;
    draws_the_box(s, "footer").unwrap();
    // The name, in `text-small-regular`, inset by `spacing-small-lvl5-inset`.
    let name = s.text_node("footer").unwrap();
    let (font_size, _, _) = s.text_styles[&name];
    let tokens = support::tokens_of(&support::document().0, &[]).0;
    let Some(nsh_core::element::Token::Typography(small)) =
        tokens.role_by_name("text-small-regular")
    else {
        panic!("text-small-regular");
    };
    assert_eq!(font_size, small.size.0);
    let inset = support::number("spacing-small-lvl5-inset");
    assert!(s.origins[&name].0 >= inset, "{:?}", s.origins[&name]);
}

#[test]
fn a_slot_holding_content_draws_nothing_of_its_own() {
    let app = start();
    let s = &app.screen;
    assert!(s.text_node("filled").is_none(), "{:?}", s.shown());
    let host = s.around("Content").unwrap();
    let style = s.styles.get(&host).cloned().unwrap_or_default();
    assert_eq!(style.borders.widths.top.0, 0.0);
    assert_eq!(style.background_color, Color::default());
    // Laid out as the text: no least size, no inset.
    assert_eq!(s.frames[&host], s.frames[&s.text_node("Content").unwrap()]);
}

#[test]
fn a_slot_that_empties_draws_the_box() {
    let mut app = start();
    assert!(app.screen.text_node("toggled").is_none());
    app.press("Toggle");
    assert!(
        app.wait(|s| s.text_node("toggled").is_some() && draws_the_box(s, "toggled").is_ok()),
        "{:?}",
        app.screen
            .text_node("toggled")
            .map(|_| draws_the_box(&app.screen, "toggled"))
    );
}
