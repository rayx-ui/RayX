//! `AutoLayout` reads roles only: for every gap and inset, each field of its spacing is the value
//! of exactly the role its prop names (`spacing-gap<n>`, `spacing-inset<n>`, 0 for `none`), under
//! RayX's theme and under another theme with token names of its own: `tests/fixtures/other_theme.json`
//! with the sixteen spacing roles mapped onto tokens of its own, each a value distinct from every
//! other and from RayX's. A mounted `AutoLayout` lays out with the other theme's values.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::sync::Arc;

use nsh_core::element::{Root, Token, Tokens, tokens};
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::auto_layout::{
    AutoLayoutDirection, Look, SpacingGap, SpacingInset, auto_layout,
};
use support::Running;

const OTHER: &str = include_str!("fixtures/other_theme.json");

/// The other theme with RayX's spacing roles mapped onto tokens of its own: `spacing-gap<n>` onto
/// `layout-gap-<n>` (`2n + 1`), `spacing-inset<n>` onto `layout-pad-<n>` (`3n + 20`).
fn other() -> String {
    let mut json: serde_json::Value = serde_json::from_str(OTHER).unwrap();
    for n in 1..=8_u32 {
        for (role, token, value) in [
            (
                format!("spacing-gap{n}"),
                format!("layout-gap-{n}"),
                2 * n + 1,
            ),
            (
                format!("spacing-inset{n}"),
                format!("layout-pad-{n}"),
                3 * n + 20,
            ),
        ] {
            json["tokens"][&token] = serde_json::json!({
                "kind": "number",
                "value": value,
                "path": format!("layout.{token}"),
            });
            json["roles"][&role] = serde_json::Value::String(token);
        }
    }
    serde_json::to_string(&json).unwrap()
}

/// Every case: a gap, the wrap gap (none: the gap's), and the padding, top, right, bottom, left.
fn cases() -> Vec<(SpacingGap, Option<SpacingGap>, [SpacingInset; 4])> {
    let mut out = Vec::new();
    for (i, gap) in SpacingGap::ALL.into_iter().enumerate() {
        for inset in SpacingInset::ALL {
            let other = SpacingInset::ALL[(i + 3) % 9];
            let wrap = SpacingGap::ALL[(i + 5) % 9];
            out.push((gap, None, [inset; 4]));
            out.push((gap, Some(wrap), [inset, other, inset, other]));
        }
    }
    out
}

/// The value of `role` (a number), 0 for none.
fn value_of(tokens: &Tokens, role: Option<&str>) -> f32 {
    match role {
        None => 0.0,
        Some(role) => match tokens.role_by_name(role) {
            Some(Token::Number(n)) => n,
            other => panic!("{role}: {other:?}"),
        },
    }
}

/// Every field of every case's look whose value is not its role's, read from `tokens`.
fn mismatches(looks: &[Look], tokens: &Tokens) -> Vec<String> {
    let gap = |g: SpacingGap| g.role().map(|r| r.name());
    let inset = |p: SpacingInset| p.role().map(|r| r.name());
    let mut wrong = Vec::new();
    for ((g, w, padding), look) in cases().into_iter().zip(looks) {
        let expected = Look {
            gap: value_of(tokens, gap(g)),
            wrap_gap: value_of(tokens, gap(w.unwrap_or(g))),
            padding: padding.map(|p| value_of(tokens, inset(p))),
        };
        if *look != expected {
            wrong.push(format!(
                "{g:?} {w:?} {padding:?}: {look:?}, not {expected:?}"
            ));
        }
    }
    wrong
}

thread_local! {
    static SEEN: RefCell<Option<(Vec<Look>, Arc<Tokens>)>> = const { RefCell::new(None) };
}

/// Every case's look where the app is, and the tokens there; and an `AutoLayout` with a gap and
/// padding.
#[component]
fn Probe() -> impl IntoElement {
    effect(move || {
        let looks = cases()
            .into_iter()
            .map(|(g, w, p)| Look::of(g, w, p))
            .collect();
        let tokens = tokens();
        SEEN.with(|seen| *seen.borrow_mut() = Some((looks, tokens)));
    });
    view().items_start().child(
        auto_layout()
            .direction(AutoLayoutDirection::Horizontal)
            .gap(SpacingGap::Gap3)
            .padding(SpacingInset::Inset2)
            .padding_top(Some(SpacingInset::Inset7))
            .child(text("P1"))
            .child(text("P2")),
    )
}

fn start(root: Root) -> (Running, Vec<Look>, Arc<Tokens>) {
    SEEN.with(|seen| *seen.borrow_mut() = None);
    let mut app = Running::start(App::Rust(root));
    assert!(app.wait(|s| s.text_frame("P2").is_some()));
    let (looks, tokens) = SEEN.with(|seen| seen.borrow().clone()).expect("looks");
    (app, looks, tokens)
}

#[test]
fn under_rayx_theme_every_field_is_its_roles_value() {
    let (_app, looks, tokens) = start(Root::of::<Probe>());
    assert_eq!(
        tokens.document().and_then(|d| d.name.as_deref()),
        Some("rayx")
    );
    assert_eq!(mismatches(&looks, &tokens), Vec::<String>::new());
}

#[test]
fn under_another_theme_every_field_is_exactly_its_roles_value() {
    let text = other();
    let read = nsh_theme::read(&text);
    assert_eq!(read.diagnostics, Vec::new());
    // Each of the sixteen roles has a value of its own there, none RayX's.
    let (theirs, _) = support::tokens_of(&Arc::new(read.document), &[]);
    let mut values = Vec::new();
    for n in 1..=8 {
        for role in [format!("spacing-gap{n}"), format!("spacing-inset{n}")] {
            let value = value_of(&theirs, Some(&role));
            assert_ne!(value, support::number(&role), "{role} is RayX's value");
            values.push(value);
        }
    }
    values.sort_by(f32::total_cmp);
    values.dedup();
    assert_eq!(values.len(), 16, "one value per role");

    let (app, looks, tokens) = start(Root::of::<Probe>().theme(text));
    assert_eq!(
        tokens.document().and_then(|d| d.name.as_deref()),
        Some("other")
    );
    assert_eq!(mismatches(&looks, &tokens), Vec::<String>::new());

    // The mounted AutoLayout: inset7 at the top, inset2 on the left, gap3 between.
    let s = &app.screen;
    let (p1, p2) = (s.text_node("P1").unwrap(), s.text_node("P2").unwrap());
    let number = |role: &str| value_of(&tokens, Some(role));
    assert_eq!(
        s.origins[&p1],
        (number("spacing-inset2"), number("spacing-inset7"))
    );
    assert_eq!(
        s.origins[&p2].0,
        s.origins[&p1].0 + s.frames[&p1].0 + number("spacing-gap3")
    );
}
