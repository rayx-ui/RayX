//! RayX's `Button`'s slots in design mode, headless: in an app a design tool started (NSH's design
//! host connected, so `design_mode()` holds), a shown slot (`showSlotLeft`, `showSlotRight`) is
//! there while empty, as RayX's `Slot`'s box at the size `Button` gives the place (the level's
//! `adornment-size` square), so the button keeps its height; a slot its `show*` prop turns off is
//! not made; a filled slot is laid out as in the running app; and NSH's dev tree has each slot's
//! marker with its JS name (`slotLeft`, `slotRight`), placed by the button (`DevNode::place`), and
//! no marker for the icons. Every test of this group runs in design mode: the process connects as
//! a design host once, to a tool that listens and never answers, as `slot_design.rs` does.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::sync::OnceLock;
use std::time::Duration;

use nsh_core::BorderStyle;
use nsh_core::element::{Root, dev_tree, spawn, timer};
use nsh_core::prelude::*;
use nsh_core::runtime::design::{self, DesignHooks};
use nsh_engine::App;
use rayx_components::button::button;
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

/// Reads NSH's dev tree once the buttons are laid out, and shows the slot markers it has as a
/// text: `places <export of the node that placed it>:<name> ...`, sorted.
#[component]
fn Places() -> impl IntoElement {
    let read = signal(String::from("reading"));
    spawn(async move {
        timer(Duration::from_millis(50)).await;
        let tree = dev_tree().await;
        let mut places: Vec<String> = tree
            .nodes
            .iter()
            .filter_map(|node| {
                let place = node.place.as_ref()?;
                let by = tree
                    .nodes
                    .iter()
                    .find(|n| Some(n.id) == place.of)
                    .and_then(|n| n.component.as_deref())
                    .unwrap_or("none");
                Some(format!("{by}:{}", place.name))
            })
            .collect();
        places.sort();
        read.set(format!("places {}", places.join(" ")));
    });
    text(move || read.get())
}

#[component]
fn Buttons() -> impl IntoElement {
    view()
        .items_start()
        .gap_2()
        .child(button().text("Plain"))
        .child(
            button()
                .text("Empty")
                .icon_left(text("+"))
                .show_slot_left(true)
                .show_slot_right(true),
        )
        .child(button().text("Off").slot_left(text("7")))
        .child(
            button()
                .text("Left")
                .show_slot_left(true)
                .slot_left(text("3")),
        )
        .child(places())
}

fn start() -> Running {
    design_host();
    let mut app = Running::start(App::Rust(Root::of::<Buttons>()));
    assert!(
        app.wait(|s| s.shown().iter().any(|t| t.starts_with("places "))),
        "{:?}",
        app.screen.shown()
    );
    app
}

/// The frame of the button labelled `label`: width, height.
fn button_size(screen: &Screen, label: &str) -> (f32, f32) {
    screen.frames[&screen.button(label).expect(label)]
}

#[test]
fn an_empty_shown_slot_is_slots_box_at_the_adornment_square() {
    let app = start();
    let s = &app.screen;
    // Each empty slot draws `Slot`'s box (its name in it), 18 square on Desktop.
    for name in ["slotLeft", "slotRight"] {
        let mark = s
            .around(name)
            .unwrap_or_else(|| panic!("no box {name}: {:?}", s.shown()));
        assert_eq!(s.frames[&mark], (18.0, 18.0), "{name}");
        let style = &s.styles[&mark];
        assert_eq!(style.borders.style, BorderStyle::Dashed, "{name}");
        assert_eq!(s.parents[&mark], s.button("Empty").unwrap(), "{name}");
    }
    // The button keeps its height.
    assert_eq!(button_size(s, "Empty").1, button_size(s, "Plain").1);
}

#[test]
fn a_slot_its_show_prop_turns_off_is_not_made() {
    let app = start();
    let s = &app.screen;
    let off = s.button("Off").unwrap();
    // The label only: no slot, filled or not.
    assert_eq!(s.children(off), [s.text_node("Off").unwrap()]);
    assert!(s.text_node("7").is_none());
}

#[test]
fn a_filled_slot_is_laid_out_as_in_the_running_app() {
    let app = start();
    let s = &app.screen;
    // As `button_slots.rs` measures it in the running app.
    let around = s.around("3").unwrap();
    assert_eq!(s.frames[&around], (18.0, 18.0));
    assert_eq!(button_size(s, "Left"), button_size_running("Left"));
}

/// The size `button_slots.rs` measures for the button labelled `label` in the running app.
fn button_size_running(label: &str) -> (f32, f32) {
    match label {
        "Left" => (LEFT_WIDTH, HEIGHT),
        _ => unreachable!("{label}"),
    }
}

/// The running app's numbers (`button_slots.rs`).
const LEFT_WIDTH: f32 = 79.0;
const HEIGHT: f32 = 32.0;

#[test]
fn the_dev_tree_has_each_slots_marker_by_its_js_name_placed_by_the_button() {
    let app = start();
    let places = app
        .screen
        .shown()
        .into_iter()
        .find(|t| t.starts_with("places "))
        .unwrap();
    // Empty's two, Left's one; Off makes none; the icons are no slots.
    assert_eq!(
        places,
        "places Button:slotLeft Button:slotLeft Button:slotRight"
    );
}
