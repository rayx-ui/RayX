//! RayX's `Button` in a Rust view, headless: its height, padding and radius come from the size's
//! level 5, its fill from the variant's role, its slots and icons show as their props say,
//! presses reach `on_press` unless it is disabled, and a new theme (Touch) resizes it in place.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::Color;
use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::button::{ButtonVariant, button};
use rayx_components::theme::{Accent, ComponentSize, Density, RayXTheme};
use support::{Running, Screen};

#[component]
fn Buttons() -> impl IntoElement {
    let presses = signal(0_u32);
    let count = move || presses.update(|n| *n += 1);
    view()
        .items_start()
        .gap_2()
        .child(
            button()
                .text("Save")
                .variant(ButtonVariant::Primary)
                .on_press(count),
        )
        .child(button().text("Small").size(ComponentSize::Small))
        .child(
            button()
                .text("Large")
                .size(ComponentSize::Large)
                .outlined(true),
        )
        .child(button().text("Quiet").variant(ButtonVariant::Ghost))
        .child(button().text("Off").disabled(true).on_press(count))
        .child(
            button()
                .text("Inbox")
                .icon_left(text("+"))
                .show_slot_right(true)
                .slot_right(text("3")),
        )
        .child(
            button()
                .text("Plain")
                .icon_left(text("x"))
                .show_left_icon(false)
                .slot_left(text("y")),
        )
        .child(
            button()
                .text("Touch")
                .on_press(|| set_theme(RayXTheme::new(Accent::Sky, Density::Touch))),
        )
        .child(text(move || format!("presses {}", presses.get())))
}

fn start() -> Running {
    let mut app = Running::start(App::Rust(Root::of::<Buttons>()));
    assert!(app.wait(|s| {
        s.pressable("Touch")
            .is_some_and(|id| s.frames.contains_key(&id))
    }));
    app
}

fn frame(screen: &Screen, label: &str) -> (f32, f32) {
    let id = screen.button(label).expect(label);
    *screen.frames.get(&id).expect("laid out")
}

fn fill(screen: &Screen, label: &str) -> Color {
    let id = screen.button(label).expect(label);
    screen.styles.get(&id).expect("styled").background_color
}

#[test]
fn a_buttons_size_is_its_level_5_and_its_look_its_variant() {
    let app = start();
    let screen = &app.screen;
    // Desktop's lvl5: small 24, default 32, large 40.
    assert_eq!(frame(screen, "Small").1, 24.0);
    assert_eq!(frame(screen, "Save").1, 32.0);
    assert_eq!(frame(screen, "Large").1, 40.0);
    // The label hugs: lvl5/inset on both sides of it.
    let save = screen.button("Save").unwrap();
    assert_eq!(
        screen.pressable("Save"),
        Some(save),
        "the button's root takes the presses"
    );
    let label = screen.text_node("Save").unwrap();
    let (width, _) = frame(screen, "Save");
    let label_width = screen.frames.get(&label).unwrap().0;
    assert_eq!(width, label_width + 2.0 * 12.0, "default lvl5/inset is 12");

    let theme = RayXTheme::default();
    let colors = &theme.theme.colors;
    let style = screen.styles.get(&save).unwrap();
    assert_eq!(style.border_radius.0, 8.0, "size/default/borders/radius");
    assert_eq!(
        fill(screen, "Save"),
        Color::from(colors.roles.primary.bg.light)
    );
    assert_eq!(
        fill(screen, "Small"),
        Color::from(colors.roles.neutral.bg.light)
    );
    assert_eq!(
        fill(screen, "Quiet"),
        Color::from(AdaptiveColor::TRANSPARENT.light),
        "a ghost button has no fill at rest"
    );
    assert_eq!(
        fill(screen, "Off"),
        Color::from(colors.interaction.disabled_bg.light)
    );
    // `outlined`: borders/outlineWidth in surfaces/border.
    let large = screen.styles.get(&screen.button("Large").unwrap()).unwrap();
    assert_eq!(large.borders.widths.top.0, 1.0);
    assert_eq!(
        large.borders.colors[0],
        Color::from(colors.surfaces.border.light)
    );
    assert_eq!(style.borders.widths.top.0, 0.0, "not outlined");
}

#[test]
fn icons_and_slots_show_as_their_props_say() {
    let app = start();
    let shown = app.screen.shown();
    // `iconLeft` shows by default once given; `slotRight` with `showSlotRight`.
    assert!(shown.contains(&"+".to_string()), "{shown:?}");
    assert!(shown.contains(&"3".to_string()), "{shown:?}");
    // `showLeftIcon: false` hides the icon; `slotLeft` without `showSlotLeft` stays hidden.
    assert!(!shown.contains(&"x".to_string()), "{shown:?}");
    assert!(!shown.contains(&"y".to_string()), "{shown:?}");
    // The icon's box is lvl5/adornmentSize square (18 on Desktop's default size).
    let plus = app.screen.text_node("+").unwrap();
    let holder = *app.screen.parents.get(&plus).unwrap();
    assert_eq!(app.screen.frames.get(&holder), Some(&(18.0, 18.0)));
}

#[test]
fn presses_reach_on_press_unless_disabled() {
    let mut app = start();
    app.press("Save");
    assert!(
        app.wait(|s| s.text_node("presses 1").is_some()),
        "{:?}",
        app.screen.shown()
    );
    // A disabled button hears no presses at all.
    let off = app.screen.button("Off").unwrap();
    assert!(app.screen.pressable("Off").is_none());
    app.press_node(off);
    app.press("Save");
    assert!(
        app.wait(|s| s.text_node("presses 2").is_some()),
        "{:?}",
        app.screen.shown()
    );
    assert!(
        app.screen.text_node("presses 3").is_none(),
        "the disabled button took a press"
    );
}

#[test]
fn a_new_theme_resizes_the_buttons_in_place() {
    let mut app = start();
    let save = app.screen.button("Save").unwrap();
    app.press("Touch");
    // Touch's lvl5: default 48, small 40; the same views, not new ones.
    assert!(app.wait(|s| s.frames.get(&save).is_some_and(|f| f.1 == 48.0)));
    assert_eq!(app.screen.button("Save"), Some(save));
    assert_eq!(frame(&app.screen, "Small").1, 40.0);
    let sky = RayXTheme::new(Accent::Sky, Density::Touch);
    assert!(app.wait(|s| fill(s, "Save") == Color::from(sky.theme.colors.roles.primary.bg.light)));
}
