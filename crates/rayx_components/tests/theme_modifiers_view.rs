//! RayX's modes as the document's modifiers in a Rust view, headless: each starts at its default
//! and RayX sets none itself, the app's `set()` of a context changes that mode's tokens only, Light
//! and Dark follow the appearance in every context, and a theme scope's modifiers change its
//! subtree only. The expected values are the theme document's.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::sync::Arc;

use nsh_core::element::{Root, ScopeModifiers, Tokens};
use nsh_core::platform::Appearance;
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::theme::keys::theme::colors::roles::primary::BG;
use rayx_components::theme::modifiers::{Accent, Density, Os};
use rayx_components::theme::value;
use support::{Document, Running, Screen, mismatches};

/// The tokens and the contexts picked, each time they changed.
type Seen = (Arc<Tokens>, Option<Accent>, Option<Density>, Option<Os>);

thread_local! {
    static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
}

#[component]
fn Modes() -> impl IntoElement {
    effect(move || {
        let seen = (
            tokens(),
            Accent::current(),
            Density::current(),
            Os::current(),
        );
        SEEN.with(|s| s.borrow_mut().push(seen));
    });
    view()
        .child(view().bg(move || value(BG)).child(text("Outside")))
        .child(
            theme_scope()
                .modifiers(Some(ScopeModifiers::from(Accent::Blue)))
                // Built where the scope places it, so inside it (a view built before is built
                // outside the scope).
                .child(scoped_panel()),
        )
        .child(
            nsh_base::button()
                .child(text("Sky"))
                .on_press(|| Accent::Sky.set()),
        )
        .child(
            nsh_base::button()
                .child(text("Touch"))
                .on_press(|| Density::Touch.set()),
        )
        .child(
            nsh_base::button()
                .child(text("Macos"))
                .on_press(|| Os::Macos.set()),
        )
        .child(
            nsh_base::button()
                .child(text("Dark"))
                .on_press(|| set_color_scheme(Appearance::Dark)),
        )
}

#[component]
fn ScopedPanel() -> impl IntoElement {
    view().bg(move || value(BG)).child(text("Scoped"))
}

fn bg(screen: &Screen, label: &str) -> Option<nsh_core::Color> {
    let node = screen.text_node(label)?;
    let panel = screen.parents.get(&node)?;
    screen.styles.get(panel).map(|s| s.background_color)
}

fn last() -> Seen {
    SEEN.with(|s| s.borrow().last().cloned())
        .expect("tokens seen")
}

fn start() -> Running {
    SEEN.with(|s| s.borrow_mut().clear());
    let mut app = Running::start(App::Rust(Root::of::<Modes>()));
    assert!(app.wait(|s| bg(s, "Scoped").is_some() && s.pressable("Dark").is_some()));
    app
}

/// Waits until the last tokens seen were read with `accent`, `density` and `os`.
fn wait_for(app: &mut Running, accent: Accent, density: Density, os: Os) {
    let picked = |_: &Screen| {
        let (_, a, d, o) = last();
        (a, d, o) == (Some(accent), Some(density), Some(os))
    };
    assert!(app.wait(picked), "{:?}", (last().1, last().2, last().3));
}

#[test]
fn each_mode_starts_at_its_default_and_rayx_sets_none() {
    let _app = start();
    let doc = Document::load();
    assert_eq!(
        (Accent::DEFAULT, Density::DEFAULT, Os::DEFAULT),
        (Accent::Cyan, Density::Desktop, Os::Windows)
    );
    let (tokens, accent, density, os) = last();
    assert_eq!(
        (accent, density, os),
        (
            Some(Accent::Cyan),
            Some(Density::Desktop),
            Some(Os::Windows)
        )
    );
    assert_eq!(mismatches(&tokens, &doc, &[]), Vec::<String>::new());
    // Nothing picked another context while the app started.
    assert!(
        SEEN.with(|s| s.borrow().iter().all(|(_, a, d, o)| (*a, *d, *o)
            == (
                Some(Accent::Cyan),
                Some(Density::Desktop),
                Some(Os::Windows)
            )))
    );
    assert_eq!(
        Accent::ALL.iter().map(|a| a.name()).collect::<Vec<_>>(),
        doc.contexts("accent")
    );
    assert_eq!(
        Os::ALL.iter().map(|o| o.name()).collect::<Vec<_>>(),
        doc.contexts("os")
    );
}

#[test]
fn a_context_the_app_sets_changes_its_modes_tokens_only() {
    let mut app = start();
    let doc = Document::load();

    app.press("Sky");
    wait_for(&mut app, Accent::Sky, Density::Desktop, Os::Windows);
    assert_eq!(
        mismatches(&last().0, &doc, &[("accent", "sky")]),
        Vec::<String>::new()
    );

    app.press("Touch");
    wait_for(&mut app, Accent::Sky, Density::Touch, Os::Windows);
    let picked = [("accent", "sky"), ("density", "touch")];
    assert_eq!(mismatches(&last().0, &doc, &picked), Vec::<String>::new());

    app.press("Macos");
    wait_for(&mut app, Accent::Sky, Density::Touch, Os::Macos);
    let picked = [("accent", "sky"), ("density", "touch"), ("os", "macos")];
    assert_eq!(mismatches(&last().0, &doc, &picked), Vec::<String>::new());
}

#[test]
fn each_context_changes_only_tokens_of_its_collections() {
    let doc = Document::load();
    let paths: std::collections::HashMap<String, String> = {
        let json: serde_json::Value =
            serde_json::from_str(rayx_components::theme::DOCUMENT).unwrap();
        json["tokens"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(n, d)| (n.clone(), d["path"].as_str().unwrap().to_owned()))
            .collect()
    };
    for (modifier, roots) in [
        ("accent", &["theme.colors.", "theme.effects."][..]),
        ("density", &["platform.density."][..]),
        ("os", &["platform.os."][..]),
    ] {
        for context in doc.contexts(modifier) {
            for (name, _) in doc.tokens() {
                if doc.value(&name, &[(modifier, &context)]) != doc.value(&name, &[]) {
                    let path = &paths[&name];
                    assert!(
                        roots.iter().any(|r| path.starts_with(r)),
                        "{modifier} {context} changes {name} ({path})"
                    );
                }
            }
        }
    }
}

#[test]
fn light_and_dark_follow_the_appearance_in_a_context() {
    let mut app = start();
    let doc = Document::load();
    app.press("Sky");
    wait_for(&mut app, Accent::Sky, Density::Desktop, Os::Windows);
    let (light, dark) = doc.color("roles-primary-bg", &[("accent", "sky")]);
    assert!(app.wait(|s| bg(s, "Outside") == Some(light)));
    app.press("Dark");
    assert!(
        app.wait(|s| bg(s, "Outside") == Some(dark)),
        "{:?}",
        bg(&app.screen, "Outside")
    );
}

#[test]
fn a_scopes_modifiers_change_its_subtree_only() {
    let mut app = start();
    let doc = Document::load();
    let (blue, _) = doc.color("roles-primary-bg", &[("accent", "blue")]);
    let (cyan, _) = doc.color("roles-primary-bg", &[]);
    let (sky, _) = doc.color("roles-primary-bg", &[("accent", "sky")]);
    assert!(
        app.wait(|s| bg(s, "Scoped") == Some(blue)),
        "{:?}",
        bg(&app.screen, "Scoped")
    );
    assert_eq!(bg(&app.screen, "Outside"), Some(cyan));

    app.press("Sky");
    assert!(
        app.wait(|s| bg(s, "Outside") == Some(sky)),
        "{:?}",
        bg(&app.screen, "Outside")
    );
    assert_eq!(
        bg(&app.screen, "Scoped"),
        Some(blue),
        "the scope keeps its own"
    );
}
