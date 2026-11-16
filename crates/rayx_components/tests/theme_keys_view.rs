//! RayX's typed keys in a Rust view, headless: a key reads its token's value, a binding on a key
//! follows Light and Dark, NSH's custom-theme API (`update_tokens` with `color!`, `reset_tokens`)
//! changes and restores what it reads, and `bind_text_style` (`text_style_keys!`) keeps a text's
//! style on its tokens as a modifier changes them. The expected values are the theme document's.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;

use nsh_core::element::{Root, Tokens};
use nsh_core::geometry::Points;
use nsh_core::platform::Appearance;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::color;
use rayx_components::text_style_keys;
use rayx_components::theme::keys::platform::density::{borders, size, typography};
use rayx_components::theme::keys::theme::colors::roles::primary::BG;
use rayx_components::theme::keys::theme::colors::surfaces::APP;
use rayx_components::theme::modifiers::Density;
use rayx_components::theme::{ComponentSize, bind_text_style, value};
use support::{Document, Running, Screen, hex};

/// What the keys read, once the view was made.
#[derive(Clone, Debug, Default)]
struct Read {
    bg: Option<AdaptiveColor>,
    height: Option<Points>,
    ring: Option<f32>,
    font: Option<SharedString>,
    by_size: Option<Points>,
}

thread_local! {
    static READ: RefCell<Read> = RefCell::new(Read::default());
}

#[component]
fn Keyed() -> impl IntoElement {
    READ.with(|r| {
        *r.borrow_mut() = Read {
            bg: BG.get(),
            height: size::default::lvl5::HEIGHT.get(),
            ring: borders::FOCUS_RING_WIDTH.get(),
            font: typography::FONT_FAMILY_PRIMARY.get(),
            by_size: ComponentSize::Large.keys().level(5).height.get(),
        };
    });
    view()
        // `surfaces-app` differs between Light and Dark.
        .child(view().bg(move || value(APP)).child(text("Primary")))
        .child(bind_text_style(
            text("Styled"),
            text_style_keys!(typography::h3),
        ))
        .child(
            nsh_base::button()
                .child(text("Touch"))
                .on_press(|| Density::Touch.set()),
        )
        .child(
            nsh_base::button()
                .child(text("Dark"))
                .on_press(|| set_color_scheme(Appearance::Dark)),
        )
        .child(
            nsh_base::button().child(text("Brand")).on_press(|| {
                update_tokens(Tokens::default().with(APP, color!("#7c3aed", "#a78bfa")))
            }),
        )
        .child(
            nsh_base::button()
                .child(text("Reset"))
                .on_press(|| reset_tokens([APP.name()])),
        )
}

fn bg(screen: &Screen) -> Option<nsh_core::Color> {
    let label = screen.text_node("Primary")?;
    let panel = screen.parents.get(&label)?;
    screen.styles.get(panel).map(|s| s.background_color)
}

fn start() -> Running {
    let mut app = Running::start(App::Rust(Root::of::<Keyed>()));
    assert!(app.wait(|s| bg(s).is_some() && s.pressable("Reset").is_some()));
    app
}

#[test]
fn a_key_reads_its_tokens_value() {
    let _app = start();
    let doc = Document::load();
    let read = READ.with(|r| r.borrow().clone());
    assert_eq!(BG.name(), "roles-primary-bg");
    let bg = read.bg.expect("roles-primary-bg");
    assert_eq!(
        (
            nsh_core::Color::from(bg.light),
            nsh_core::Color::from(bg.dark)
        ),
        doc.color("roles-primary-bg", &[])
    );
    assert_eq!(
        read.height.unwrap().0,
        doc.number("spacing-size-default-lvl5-height", &[])
    );
    assert_eq!(read.ring, Some(doc.number("borders-focus-ring-width", &[])));
    assert_eq!(read.font.as_deref(), Some("Inter"));
    assert_eq!(
        read.by_size.unwrap().0,
        doc.number("spacing-size-large-lvl5-height", &[]),
        "a size's level keys"
    );
}

#[test]
fn a_binding_follows_the_appearance_and_the_apps_tokens() {
    let mut app = start();
    let doc = Document::load();
    let (light, dark) = doc.color("surfaces-app", &[]);
    assert_ne!(light, dark, "a token whose appearances differ");
    assert_eq!(bg(&app.screen), Some(light));

    app.press("Dark");
    assert!(app.wait(|s| bg(s) == Some(dark)), "{:?}", bg(&app.screen));

    // `update_tokens` with `color!`: Dark's value of the app's color.
    app.press("Brand");
    assert!(
        app.wait(|s| bg(s) == Some(hex("#a78bfaff"))),
        "{:?}",
        bg(&app.screen)
    );

    // `reset_tokens`: the document's value again.
    app.press("Reset");
    assert!(app.wait(|s| bg(s) == Some(dark)), "{:?}", bg(&app.screen));
}

fn styled(screen: &Screen) -> Option<(f32, f32, f32)> {
    let node = screen.text_node("Styled")?;
    screen.text_styles.get(&node).copied()
}

#[test]
fn a_bound_text_style_follows_its_tokens() {
    let mut app = start();
    let doc = Document::load();
    let h3 = |picked: &[(&str, &str)]| {
        (
            doc.number("text-typography-h3-font-size", picked),
            doc.number("spacing-typography-h3-line-height", picked),
            doc.number("spacing-typography-h3-letter-spacing", picked),
        )
    };
    let desktop = h3(&[]);
    let touch = h3(&[("density", "touch")]);
    assert_ne!(desktop, touch, "h3 differs between the densities");
    assert!(
        app.wait(|s| styled(s) == Some(desktop)),
        "{:?}",
        styled(&app.screen)
    );

    // The same text, its style following the density's tokens.
    let node = app.screen.text_node("Styled").unwrap();
    app.press("Touch");
    assert!(
        app.wait(|s| styled(s) == Some(touch)),
        "{:?}",
        styled(&app.screen)
    );
    assert_eq!(app.screen.text_node("Styled"), Some(node));
}
