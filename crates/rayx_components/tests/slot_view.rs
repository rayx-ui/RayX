//! RayX's `Slot` in a Rust view, headless, outside design mode: empty, it shows nothing and takes
//! no space, not even its parent's gap, also when it empties after showing content; holding one
//! element, that element is laid out as it is without the slot; what the component's own code
//! puts in it is the default content, which what the component's user passes replaces (a slot's
//! place given to the `Slot` as its child); two things given to one slot make a dev build warn
//! once, naming the slot, and both still show.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::button::button;
use rayx_components::slot::{slot, warned};
use support::{Running, Screen};

/// A card whose footer slot shows its own default content until the app passes one.
#[derive(Debug, Clone, PartialEq, Props)]
struct CardProps {
    /// The card's footer.
    footer: Prop<Option<Slot>>,
}

/// A card: its title, and its footer in a `Slot`.
#[component]
fn Card(props: CardProps) -> impl IntoElement {
    view().child(text("card")).child(
        slot()
            .name("footer")
            .child(props.footer.place(|f| f))
            .child(when(
                move || props.footer.get().is_none(),
                || text("Default footer"),
            )),
    )
}

/// A column of `A<n>`, what `middle` adds, and `B<n>`, ten points apart.
fn column(n: u32, middle: Option<AnyElement>) -> View {
    view()
        .gap(px(10.0))
        .child(text(format!("A{n}")))
        .children(middle)
        .child(text(format!("B{n}")))
}

#[component]
fn Slots() -> impl IntoElement {
    let shown = signal(true);
    view()
        .flex_row()
        .items_start()
        .gap(px(8.0))
        // An empty slot, and the same column without it.
        .child(column(1, Some(slot().name("empty").into_element())))
        .child(column(2, None))
        // A slot holding one text, and the same column with the text in its place.
        .child(column(
            3,
            Some(slot().name("one").child(text("C3")).into_element()),
        ))
        .child(column(4, Some(text("C4").into_element())))
        // A slot whose content comes and goes.
        .child(column(
            5,
            Some(
                slot()
                    .name("toggled")
                    .child(when(move || shown.get(), || text("T5")))
                    .into_element(),
            ),
        ))
        .child(
            button()
                .text("Toggle")
                .on_press(move || shown.update(|s| *s = !*s)),
        )
        // Default content, and content the app passes.
        .child(card())
        .child(card().footer(text("Custom footer")))
        // Two things in one slot.
        .child(slot().name("pair-rust").child(text("x1")).child(text("x2")))
}

fn start() -> Running {
    let mut app = Running::start(App::Rust(Root::of::<Slots>()));
    assert!(
        app.wait(|s| {
            ["B1", "B2", "C3", "C4", "B5", "x1", "x2", "Custom footer"]
                .iter()
                .all(|label| s.text_frame(label).is_some())
                && s.pressable("Toggle").is_some()
        }),
        "{:?}",
        app.screen.shown()
    );
    app
}

/// Where `label` is in its column (the view around `A<n>`): x, y, width, height.
fn in_column(screen: &Screen, column: &str, label: &str) -> (f32, f32, f32, f32) {
    let around = screen.around(column).unwrap();
    let (cx, cy) = screen.absolute(around);
    let (x, y, w, h) = screen.text_frame(label).unwrap();
    (x - cx, y - cy, w, h)
}

#[test]
fn an_empty_slot_shows_nothing_and_takes_no_gap() {
    let app = start();
    let s = &app.screen;
    // `B1` is where `B2` is: one gap after `A`, none for the slot between them.
    assert_eq!(in_column(s, "A1", "B1"), in_column(s, "A2", "B2"));
    assert_eq!(
        in_column(s, "A1", "B1").1,
        in_column(s, "A1", "A1").3 + 10.0
    );
    // Outside design mode it draws nothing, its name neither.
    assert!(s.text_node("empty").is_none(), "{:?}", s.shown());
}

#[test]
fn a_slot_holding_one_element_lays_it_out_as_without_the_slot() {
    let app = start();
    let s = &app.screen;
    assert_eq!(in_column(s, "A3", "C3"), in_column(s, "A4", "C4"));
    assert_eq!(in_column(s, "A3", "B3"), in_column(s, "A4", "B4"));
    let width = |label: &str| s.frames[&s.around(label).unwrap()].0;
    assert_eq!(width("A3"), width("A4"), "the columns are as wide");
}

#[test]
fn a_slot_that_empties_takes_no_gap_again() {
    let mut app = start();
    let held = in_column(&app.screen, "A5", "B5");
    assert_eq!(held.1, in_column(&app.screen, "A4", "B4").1, "T5 shows");
    app.press("Toggle");
    assert!(
        app.wait(|s| s.text_node("T5").is_none()),
        "{:?}",
        app.screen.shown()
    );
    assert!(app.wait(|s| in_column(s, "A5", "B5") == in_column(s, "A2", "B2")));
}

#[test]
fn the_components_default_content_shows_until_the_app_passes_its_own() {
    let app = start();
    let shown = app.screen.shown();
    let count = |label: &str| shown.iter().filter(|t| t.as_str() == label).count();
    assert_eq!(count("card"), 2, "{shown:?}");
    // One card shows its default footer; the other the app's, which replaced it.
    assert_eq!(count("Default footer"), 1, "{shown:?}");
    assert_eq!(count("Custom footer"), 1, "{shown:?}");
    let custom = app.screen.around("Custom footer").unwrap();
    let default = app.screen.around("Default footer").unwrap();
    assert_ne!(
        app.screen.parents.get(&custom),
        app.screen.parents.get(&default),
        "each in its own card"
    );
}

#[test]
fn two_things_in_one_slot_warn_once_naming_it_and_both_show() {
    let app = start();
    assert!(app.screen.text_node("x1").is_some() && app.screen.text_node("x2").is_some());
    let warned = warned();
    assert_eq!(
        warned.iter().filter(|n| n.as_str() == "pair-rust").count(),
        1,
        "{warned:?}"
    );
    for one in ["empty", "one", "toggled", "footer"] {
        assert!(!warned.iter().any(|n| n == one), "{one}: {warned:?}");
    }
}
