//! RayX's `AutoLayout` in a Rust view, headless: the direction, the gap and the padding (each a
//! spacing role's value, per side over all sides), Figma's nine alignments in both directions,
//! `spaceBetween`, wrapping with its own gap between lines, and the sizing of an `AutoLayout`
//! (hug, fill, fixed) against the `AutoLayout` around it, in a row and in a column; a child that
//! is not an `AutoLayout` fills by its styles (`flex_1`, `flex-1` in TSX). The expected values
//! are those RayX's default theme gives the roles.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::auto_layout::{
    AutoLayoutAlign, AutoLayoutDirection, AutoLayoutSizing, Place, SpacingGap, SpacingInset,
    auto_layout,
};
use rayx_components::button::button;
use support::{Running, Screen, number};

use AutoLayoutDirection::{Horizontal, Vertical};
use AutoLayoutSizing::{Fill, Fixed, Hug};

/// An `AutoLayout` of `width` × `height` points.
fn fixed(width: f32, height: f32) -> rayx_components::auto_layout::AutoLayoutElement {
    auto_layout()
        .width(Fixed)
        .fixed_width(Some(width))
        .height(Fixed)
        .fixed_height(Some(height))
}

#[component]
fn Layouts() -> impl IntoElement {
    let aligned = |direction: AutoLayoutDirection, prefix: &'static str| {
        AutoLayoutAlign::ALL.map(move |align| {
            let index = AutoLayoutAlign::ALL
                .iter()
                .position(|a| *a == align)
                .unwrap();
            fixed(200.0, 100.0)
                .direction(direction)
                .align(align)
                .child(text(format!("{prefix}{index}")))
        })
    };
    view()
        .items_start()
        .gap(px(20.0))
        // Direction, gap and padding.
        .child(
            auto_layout()
                .direction(Horizontal)
                .gap(SpacingGap::Gap4)
                .padding(SpacingInset::Inset3)
                .padding_left(Some(SpacingInset::Inset5))
                .child(text("H1"))
                .child(text("H2")),
        )
        .child(
            auto_layout()
                .gap(SpacingGap::Gap2)
                .child(text("V1"))
                .child(text("V2")),
        )
        // The nine alignments, in a row and in a column.
        .children(aligned(Horizontal, "h"))
        .children(aligned(Vertical, "v"))
        // Space between, the children placed across by `align`.
        .child(
            fixed(200.0, 60.0)
                .direction(Horizontal)
                .space_between(true)
                .align(AutoLayoutAlign::Bottom)
                .child(text("S1"))
                .child(text("S2"))
                .child(text("S3")),
        )
        // Wrapping: two to a line, the lines `spacing-gap6` apart.
        .child(
            auto_layout()
                .direction(Horizontal)
                .width(Fixed)
                .fixed_width(Some(45.0))
                .wrap(true)
                .gap(SpacingGap::Gap2)
                .wrap_gap(Some(SpacingGap::Gap6))
                .child(text("W1"))
                .child(text("W2"))
                .child(text("W3"))
                .child(text("W4")),
        )
        // Sizing in a row: fill, fixed and hug side by side.
        .child(
            fixed(300.0, 100.0)
                .direction(Horizontal)
                .child(auto_layout().width(Fill).height(Fill).child(text("RF")))
                .child(
                    auto_layout()
                        .width(Fixed)
                        .fixed_width(Some(50.0))
                        .child(text("RX")),
                )
                .child(auto_layout().width(Hug).child(text("RG"))),
        )
        // Sizing in a column.
        .child(
            fixed(300.0, 200.0)
                .child(auto_layout().width(Fill).height(Fill).child(text("CF")))
                .child(
                    auto_layout()
                        .height(Fixed)
                        .fixed_height(Some(50.0))
                        .child(text("CX")),
                )
                .child(auto_layout().child(text("CG"))),
        )
        // A child that is not an `AutoLayout` fills by its styles.
        .child(
            fixed(300.0, 40.0)
                .direction(Horizontal)
                .child(button().text("Wide").flex_1())
                .child(text("BZ")),
        )
}

fn start() -> Running {
    let mut app = Running::start(App::Rust(Root::of::<Layouts>()));
    assert!(
        app.wait(|s| ["H2", "v8", "S3", "W4", "RG", "CG", "BZ"]
            .iter()
            .all(|label| s.text_frame(label).is_some())),
        "{:?}",
        app.screen.shown()
    );
    app
}

/// `label`'s frame in the `AutoLayout` around it: x, y, width, height.
fn in_layout(s: &Screen, label: &str) -> (f32, f32, f32, f32) {
    let id = s.text_node(label).unwrap();
    let (x, y) = s.origins[&id];
    let (w, h) = s.frames[&id];
    (x, y, w, h)
}

/// The size of the `AutoLayout` around `label`.
fn layout_size(s: &Screen, label: &str) -> (f32, f32) {
    s.frames[&s.around(label).unwrap()]
}

/// Whether two lengths are within a point (layout rounds to pixels).
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

#[test]
fn direction_gap_and_padding_come_from_spacing_roles() {
    let app = start();
    let s = &app.screen;
    let (gap4, gap2) = (number("spacing-gap4"), number("spacing-gap2"));
    let (inset3, inset5) = (number("spacing-inset3"), number("spacing-inset5"));
    assert!(gap4 > 0.0 && gap2 > 0.0 && inset3 > 0.0 && inset5 != inset3);

    // A row: `paddingLeft` over `padding` on the left, `padding` elsewhere.
    let h1 = in_layout(s, "H1");
    let h2 = in_layout(s, "H2");
    assert_eq!((h1.0, h1.1), (inset5, inset3));
    assert_eq!(h2.0, h1.0 + h1.2 + gap4);
    assert_eq!(h2.1, inset3);
    let (width, height) = layout_size(s, "H1");
    assert_eq!(width, inset5 + h1.2 + gap4 + h2.2 + inset3, "it hugs");
    assert_eq!(height, inset3 + h1.3.max(h2.3) + inset3);

    // A column, the default.
    let v1 = in_layout(s, "V1");
    let v2 = in_layout(s, "V2");
    assert_eq!((v1.0, v1.1), (0.0, 0.0));
    assert_eq!((v2.0, v2.1), (0.0, v1.3 + gap2));
}

#[test]
fn the_nine_alignments_place_the_children_in_both_directions() {
    let app = start();
    let s = &app.screen;
    for prefix in ["h", "v"] {
        for (index, align) in AutoLayoutAlign::ALL.iter().enumerate() {
            let label = format!("{prefix}{index}");
            let (x, y, w, h) = in_layout(s, &label);
            assert_eq!(layout_size(s, &label), (200.0, 100.0), "{label}: fixed");
            let at = |place: Place, room: f32| match place {
                Place::Start => 0.0,
                Place::Center => room / 2.0,
                Place::End => room,
            };
            let (horizontal, vertical) = align.axes();
            assert!(
                near(x, at(horizontal, 200.0 - w)) && near(y, at(vertical, 100.0 - h)),
                "{label} {align:?}: at {x}, {y}"
            );
        }
    }
}

#[test]
fn space_between_spreads_the_children_and_align_places_them_across() {
    let app = start();
    let s = &app.screen;
    let (s1, s2, s3) = (in_layout(s, "S1"), in_layout(s, "S2"), in_layout(s, "S3"));
    assert_eq!(s1.0, 0.0);
    assert!(near(s3.0 + s3.2, 200.0), "{s3:?}");
    assert!(near(s2.0 - (s1.0 + s1.2), s3.0 - (s2.0 + s2.2)), "{s2:?}");
    // `bottom`: at the bottom across the row.
    for c in [s1, s2, s3] {
        assert!(near(c.1 + c.3, 60.0), "{c:?}");
    }
}

#[test]
fn wrapping_puts_the_children_on_lines_the_wrap_gap_apart() {
    let app = start();
    let s = &app.screen;
    let (gap2, gap6) = (number("spacing-gap2"), number("spacing-gap6"));
    let (w1, w2, w3, w4) = (
        in_layout(s, "W1"),
        in_layout(s, "W2"),
        in_layout(s, "W3"),
        in_layout(s, "W4"),
    );
    assert_eq!((w1.0, w1.1), (0.0, 0.0));
    assert_eq!((w2.0, w2.1), (w1.2 + gap2, 0.0), "the first line");
    assert_eq!((w3.0, w3.1), (0.0, w1.3 + gap6), "the second line");
    assert_eq!((w4.0, w4.1), (w3.2 + gap2, w3.1));
    assert_eq!(layout_size(s, "W1"), (45.0, w1.3 + gap6 + w3.3));
}

#[test]
fn hug_fill_and_fixed_size_an_auto_layout_against_the_one_around_it() {
    let app = start();
    let s = &app.screen;
    // In a row: fill takes the width the others leave and the whole height; fixed and hug keep
    // theirs.
    let g = in_layout(s, "RG");
    assert_eq!(layout_size(s, "RX").0, 50.0);
    assert_eq!(layout_size(s, "RG"), (g.2, g.3), "hug");
    assert_eq!(layout_size(s, "RF"), (300.0 - 50.0 - g.2, 100.0), "fill");
    assert_eq!(s.text_frame("RX").unwrap().3, layout_size(s, "RX").1);

    // In a column: fill takes the height the others leave and the whole width.
    let g = in_layout(s, "CG");
    assert_eq!(layout_size(s, "CX").1, 50.0);
    assert_eq!(layout_size(s, "CG"), (g.2, g.3), "hug");
    assert_eq!(layout_size(s, "CF"), (300.0, 200.0 - 50.0 - g.3), "fill");
}

#[test]
fn a_child_that_is_no_auto_layout_fills_by_its_styles() {
    let app = start();
    let s = &app.screen;
    let z = in_layout(s, "BZ");
    let wide = s.button("Wide").unwrap();
    assert_eq!(s.frames[&wide].0, 300.0 - z.2, "flex_1 along the row");
}
