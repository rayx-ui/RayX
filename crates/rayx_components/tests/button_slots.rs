//! RayX's `Button`'s slots, headless, in the running app: `slotLeft` and `slotRight` are marked
//! with RayX's `Slot` in its view, and the button lays out exactly as before: a filled slot is a
//! box of the level's `adornment-size` height (and at least as wide) with the content centred,
//! an empty one or one its `show*` prop turns off is no node and takes no gap. The numbers are
//! those RayX's default theme gives the roles on Desktop, measured before the slots were marked.
//! (Design mode, where an empty shown slot draws its box: `button_slots_design.rs`.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::button::button;
use rayx_components::theme::ComponentSize;
use support::{Running, Screen};

#[component]
fn Buttons() -> impl IntoElement {
    view()
        .items_start()
        .gap_2()
        .child(button().text("Plain"))
        .child(
            button()
                .text("Left")
                .show_slot_left(true)
                .slot_left(text("3")),
        )
        .child(
            button()
                .text("Right")
                .show_slot_right(true)
                .slot_right(text("4")),
        )
        .child(
            button()
                .text("Both")
                .show_slot_left(true)
                .slot_left(text("5"))
                .show_slot_right(true)
                .slot_right(text("6")),
        )
        .child(
            button()
                .text("Empty")
                .show_slot_left(true)
                .show_slot_right(true),
        )
        .child(button().text("Off").slot_left(text("7")))
        .child(
            button()
                .text("Small")
                .size(ComponentSize::Small)
                .show_slot_left(true)
                .slot_left(text("8")),
        )
}

fn start() -> Running {
    let mut app = Running::start(App::Rust(Root::of::<Buttons>()));
    assert!(app.wait(|s| {
        s.text_node("8")
            .is_some_and(|id| s.frames.contains_key(&id))
    }));
    app
}

/// The frame of the button labelled `label`, in the window: x, y, width, height.
fn button_frame(screen: &Screen, label: &str) -> (f32, f32, f32, f32) {
    let id = screen.button(label).expect(label);
    let (w, h) = screen.frames[&id];
    let (x, y) = screen.absolute(id);
    (x, y, w, h)
}

/// The frame of the box around the text `content`, in the window: x, y, width, height.
fn box_frame(screen: &Screen, content: &str) -> (f32, f32, f32, f32) {
    let id = screen.around(content).expect(content);
    let (w, h) = screen.frames[&id];
    let (x, y) = screen.absolute(id);
    (x, y, w, h)
}

#[test]
fn a_filled_slot_lays_the_button_out_as_before() {
    let app = start();
    let s = &app.screen;
    // The buttons: x, y, width, height.
    assert_eq!(button_frame(s, "Plain"), (0.0, 0.0, 63.0, 32.0));
    assert_eq!(button_frame(s, "Left"), (0.0, 40.0, 79.0, 32.0));
    assert_eq!(button_frame(s, "Right"), (0.0, 80.0, 87.0, 32.0));
    assert_eq!(button_frame(s, "Both"), (0.0, 120.0, 103.0, 32.0));
    assert_eq!(button_frame(s, "Small"), (0.0, 240.0, 72.0, 24.0));
    // Each slot's box: the level's adornment-size square, its content centred in it.
    assert_eq!(box_frame(s, "3"), (12.0, 47.0, 18.0, 18.0));
    assert_eq!(s.text_frame("3"), Some((17.0, 47.0, 8.0, 18.0)));
    assert_eq!(box_frame(s, "4"), (57.0, 87.0, 18.0, 18.0));
    assert_eq!(box_frame(s, "5"), (12.0, 127.0, 18.0, 18.0));
    assert_eq!(box_frame(s, "6"), (73.0, 127.0, 18.0, 18.0));
    assert_eq!(box_frame(s, "8"), (8.0, 244.0, 16.0, 16.0));
    // The labels after the slots and the gap.
    assert_eq!(s.text_frame("Left"), Some((36.0, 47.0, 31.0, 18.0)));
    assert_eq!(s.text_frame("Both"), Some((36.0, 127.0, 31.0, 18.0)));
}

#[test]
fn an_empty_slot_or_one_its_show_prop_turns_off_is_no_node() {
    let app = start();
    let s = &app.screen;
    // `showSlotLeft` and `showSlotRight` on, nothing given: as a plain button.
    assert_eq!(button_frame(s, "Empty"), (0.0, 160.0, 63.0, 32.0));
    let empty = s.button("Empty").unwrap();
    assert_eq!(s.children(empty), [s.text_node("Empty").unwrap()]);
    // Given, `showSlotLeft` off: not made.
    assert_eq!(button_frame(s, "Off"), (0.0, 200.0, 48.0, 32.0));
    let off = s.button("Off").unwrap();
    assert_eq!(s.children(off), [s.text_node("Off").unwrap()]);
    assert!(s.text_node("7").is_none(), "{:?}", s.shown());
}
