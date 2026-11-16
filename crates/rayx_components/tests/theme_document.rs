//! RayX's theme document in NSH, headless: the library's set registers it (no theme type), every
//! token it declares has the document's value, and nsh-base's parts draw with RayX's tokens
//! through the document's roles.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::sync::Arc;

use nsh_core::Color;
use nsh_core::element::{Root, Tokens};
use nsh_core::prelude::*;
use nsh_engine::App;
use support::{Document, Running, mismatches};

thread_local! {
    static TOKENS: RefCell<Option<Arc<Tokens>>> = const { RefCell::new(None) };
    static BASE: RefCell<Option<Arc<nsh_base::Theme>>> = const { RefCell::new(None) };
}

#[component]
fn Reader() -> impl IntoElement {
    effect(move || {
        let tokens = tokens();
        TOKENS.with(|t| *t.borrow_mut() = Some(tokens));
        let base = nsh_base::theme();
        BASE.with(|b| *b.borrow_mut() = Some(base));
    });
    text("ready")
}

fn start() -> Running {
    let mut app = Running::start(App::Rust(Root::of::<Reader>()));
    assert!(app.wait(|s| s.text_node("ready").is_some()));
    app
}

#[test]
fn the_library_registers_its_document() {
    // The sets merge without a panic: no token or role of RayX's claimed twice.
    let sets = support::ui_sets();
    assert_eq!(sets.len(), 2);
    let doc = Document::load();
    assert_eq!(doc.tokens().len(), 710, "every token of the export, once");
}

#[test]
fn every_declared_token_has_the_documents_value() {
    let _app = start();
    let doc = Document::load();
    let tokens = TOKENS.with(|t| t.borrow().clone()).expect("tokens read");
    assert_eq!(mismatches(&tokens, &doc, &[]), Vec::<String>::new());
    // Declared by RayX's document, each of its kind.
    let declared = tokens.declarations().expect("declarations");
    for (name, kind) in doc.tokens() {
        assert_eq!(
            tokens.kind_name(&name),
            Some(kind.as_str()),
            "{name} is declared as {kind}"
        );
        assert!(declared.path_of(&name).is_some(), "{name} has a path");
    }
}

#[test]
fn nsh_base_draws_with_rayx_tokens_through_the_roles() {
    let _app = start();
    let doc = Document::load();
    let base = BASE.with(|b| b.borrow().clone()).expect("theme read");
    let color = |role: &str| doc.color(&doc.role(role), &[]);
    let light = |c: AdaptiveColor| Color::from(c.light);
    let dark = |c: AdaptiveColor| Color::from(c.dark);
    for (role, value) in [
        ("primary", base.primary),
        ("background", base.background),
        ("popover", base.popover),
        ("border", base.border),
        ("ring", base.ring),
        ("destructive", base.destructive),
    ] {
        assert_eq!(
            (light(value), dark(value)),
            color(role),
            "the role `{role}`"
        );
    }
    assert_eq!(
        base.radius.0,
        doc.number(&doc.role("radius"), &[]),
        "radius"
    );
    assert_eq!(base.font_sans.as_deref(), Some("Inter"), "font-sans");
}
