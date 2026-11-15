//! RayX's theme in Rust views, headless: `rayx()` in a binding and `bind_text_style` follow the
//! theme as `set_theme` changes it (Desktop to Touch), on the same views.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::Color;
use nsh_core::element::Root;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::button::button;
use rayx_components::theme::{Accent, Density, RayXTheme, bind_text_style, rayx};
use support::Running;

#[component]
fn Themed() -> impl IntoElement {
    view()
        .p(move || px(rayx().platform.density.insets.inset4))
        .bg(move || rayx().theme.colors.surfaces.panel1)
        .child(
            bind_text_style(text("Heading"), |t| {
                t.platform.density.typography.h3.clone()
            })
            .text_color(move || rayx().theme.colors.surfaces.text),
        )
        .child(
            button()
                .text("Touch")
                .on_press(|| set_theme(RayXTheme::new(Accent::Blue, Density::Touch))),
        )
}

#[test]
fn bindings_and_text_styles_follow_the_theme() {
    let mut app = Running::start(App::Rust(Root::of::<Themed>()));
    assert!(app.wait(|s| s.pressable("Touch").is_some()));
    let heading = app.screen.text_node("Heading").unwrap();
    let panel = *app.screen.parents.get(&heading).unwrap();
    let desktop = RayXTheme::default();
    let touch = RayXTheme::new(Accent::Blue, Density::Touch);
    let h3 = |t: &RayXTheme| t.platform.density.typography.h3.clone();
    assert_ne!(
        h3(&desktop).font_size,
        h3(&touch).font_size,
        "the densities' h3 differ"
    );

    let (x, _) = app.screen.origins[&heading];
    assert_eq!(x, desktop.platform.density.insets.inset4);
    assert_eq!(
        app.screen.styles[&panel].background_color,
        Color::from(desktop.theme.colors.surfaces.panel1.light)
    );
    let style = |t: &RayXTheme| {
        let h3 = h3(t);
        (h3.font_size, h3.line_height, h3.letter_spacing)
    };
    assert_eq!(app.screen.text_styles[&heading], style(&desktop));

    app.press("Touch");
    let expected = style(&touch);
    assert!(app.wait(|s| s.text_styles.get(&heading) == Some(&expected)));
    assert_eq!(
        app.screen.text_node("Heading"),
        Some(heading),
        "the same view"
    );
    assert_eq!(
        app.screen.origins[&heading].0,
        touch.platform.density.insets.inset4
    );
}
