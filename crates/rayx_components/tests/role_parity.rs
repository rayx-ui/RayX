//! The new default theme gives every role what the old one gave: under each of the 12
//! combinations of `colorMode`, `accent` and `density`, each role of
//! `tests/fixtures/role_values.json` (recorded by role name from the theme before the move to
//! NSH's theme format) resolves, through the role itself, to the recorded value: a color within
//! one step per 8-bit channel (the two generators round differently), a number exactly, a font
//! by its family, a typography by its four parts. Every role of `roles.json` is in the fixture or
//! in its list of roles left out. No value is looked up by a token's name or path.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::collections::BTreeSet;

use nsh_core::LineHeight;
use nsh_core::element::Token;
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/role_values.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("role_values.json")
}

/// Why `token` is not `expected`, if it is not.
fn mismatch(token: &Token, expected: &Value) -> Option<String> {
    let wrong = |got: String| Some(format!("expected {expected}, got {got}"));
    match (token, expected) {
        (Token::Color(color), Value::String(hex)) => {
            let got = support::rgba(*color);
            (!support::near(got, support::hex(hex)))
                .then(|| format!("expected {hex}, got #{got:08x}"))
        }
        (Token::Number(number), Value::Number(n)) => (f64::from(*number) != n.as_f64().unwrap())
            .then(|| format!("expected {n}, got {number}")),
        (Token::Font(font), Value::String(family)) => {
            (font.as_ref() != family).then(|| format!("expected {family}, got {font}"))
        }
        (Token::Typography(t), Value::Object(parts)) => {
            let number = |key: &str| parts[key].as_f64().unwrap() as f32;
            let line_height = match t.line_height {
                Some(LineHeight::Points(points)) => Some(points.0),
                _ => None,
            };
            let got = (
                t.size.0,
                line_height,
                t.letter_spacing.map(|s| s.0),
                t.font_weight,
            );
            let want = (
                number("fontSize"),
                Some(number("lineHeight")),
                Some(number("letterSpacing")),
                Some(number("fontWeight")),
            );
            (got != want).then(|| format!("expected {want:?}, got {got:?}"))
        }
        (other, _) => wrong(format!("{other:?}")),
    }
}

#[test]
fn every_recorded_role_resolves_to_its_old_value_under_every_combination() {
    let fixture = fixture();
    let (document, _) = support::document();
    let combinations = fixture["combinations"].as_array().unwrap();
    assert_eq!(combinations.len(), 12);
    let mut wrong = Vec::new();
    for combination in combinations {
        let picks: Vec<(&str, &str)> = combination["picks"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(m, c)| (m.as_str(), c.as_str().unwrap()))
            .collect();
        let (tokens, _) = support::tokens_of(&document, &picks);
        for (role, expected) in combination["values"].as_object().unwrap() {
            match tokens.role_by_name(role) {
                Some(token) => {
                    if let Some(why) = mismatch(&token, expected) {
                        wrong.push(format!("{picks:?} {role}: {why}"));
                    }
                }
                None => wrong.push(format!("{picks:?} {role}: resolves to nothing")),
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} roles differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn every_rayx_role_is_recorded_or_listed_as_left_out() {
    let fixture = fixture();
    let left_out: BTreeSet<&str> = fixture["leftOut"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let recorded: BTreeSet<&str> = fixture["combinations"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["values"].as_object().unwrap().keys().map(String::as_str))
        .collect();
    let unaccounted: Vec<String> = rayx_components::theme::roles::all()
        .into_iter()
        .map(|r| r.name)
        .filter(|name| !recorded.contains(name.as_str()) && !left_out.contains(name.as_str()))
        .collect();
    assert_eq!(unaccounted, Vec::<String>::new());
    // Only shadows and blurs are left out: the old document held them as CSS strings.
    for name in &left_out {
        assert!(
            name.starts_with("shadow-") || name.starts_with("blur-"),
            "{name}"
        );
    }
    // Button's roles, the 33 core and nsh-base roles, and the other roles are recorded.
    for role in [
        "intent-primary-bg",
        "text-default-title1-bold",
        "background",
        "font-sans",
        "radius",
        "spacing-gap1",
    ] {
        assert!(recorded.contains(role), "{role}");
    }
}
