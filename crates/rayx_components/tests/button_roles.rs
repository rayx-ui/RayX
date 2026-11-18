//! Button reads roles only (D-BUTTON): for every variant, size, `outlined` and `disabled`, each
//! field of its look is the value of exactly the role D-BUTTON's table names, under RayX's theme
//! and under `tests/fixtures/other_theme.json`, a theme with token names and paths of its own
//! whose 45 Button roles each have a value distinct from every other role of their type. A picked
//! context, `set_theme` and a patch of a mapped token change what a mounted Button draws without
//! remounting it, and a theme that leaves a role unmapped draws that value transparent or 0.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use nsh_core::LineHeight;
use nsh_core::color::Hsla;
use nsh_core::element::{
    Patch, PatchValues, Root, Token, Tokens, Typography, set_modifier, set_theme, update_tokens,
};
use nsh_core::prelude::*;
use nsh_engine::App;
use rayx_components::button::{ButtonVariant, Look, button};
use rayx_components::theme::ComponentSize;
use support::{Running, Screen, rgba};

const OTHER: &str = include_str!("fixtures/other_theme.json");

/// Every combination of Button's look props.
fn cases() -> Vec<(ButtonVariant, ComponentSize, bool, bool)> {
    let mut out = Vec::new();
    for variant in ButtonVariant::ALL {
        for size in ComponentSize::ALL {
            for outlined in [false, true] {
                for disabled in [false, true] {
                    out.push((variant, size, outlined, disabled));
                }
            }
        }
    }
    out
}

/// A look field's expected source: a role (by name), or a constant.
#[derive(Debug, Clone, PartialEq)]
enum From {
    Role(String),
    Transparent,
    Zero,
}

fn role(name: impl Into<String>) -> From {
    From::Role(name.into())
}

/// D-BUTTON's table, written out: the source of each field of the look of these props.
fn table(
    variant: ButtonVariant,
    size: ComponentSize,
    outlined: bool,
    disabled: bool,
) -> Vec<(&'static str, From)> {
    let s = size.name();
    let intent = |v: &str| {
        [
            role(format!("intent-{v}-bg")),
            role(format!("intent-{v}-bg-hover")),
            role(format!("intent-{v}-bg-active")),
            role(format!("intent-{v}-text")),
            role(format!("intent-{v}-text")),
        ]
    };
    let [mut bg, mut bg_hover, mut bg_active, mut fg, mut fg_hover] = match variant {
        ButtonVariant::Neutral => intent("neutral"),
        ButtonVariant::Primary => intent("primary"),
        ButtonVariant::Danger => intent("danger"),
        ButtonVariant::Success => intent("success"),
        ButtonVariant::Ghost => [
            From::Transparent,
            role("intent-neutral-bg-hover"),
            role("intent-neutral-bg-active"),
            role("intent-neutral-text"),
            role("intent-neutral-text"),
        ],
        ButtonVariant::Link => [
            From::Transparent,
            From::Transparent,
            From::Transparent,
            role("surface-link"),
            role("surface-link-hover"),
        ],
    };
    let ring = match variant {
        ButtonVariant::Danger => role("interaction-focus-ring-danger"),
        ButtonVariant::Success => role("interaction-focus-ring-success"),
        _ => role("interaction-focus-ring"),
    };
    let mut border = role("surface-border");
    let border_width = if outlined && variant != ButtonVariant::Link {
        role("border-width-outline")
    } else {
        From::Zero
    };
    if disabled {
        let fill = match variant {
            ButtonVariant::Ghost | ButtonVariant::Link => From::Transparent,
            _ => role("interaction-disabled-bg"),
        };
        bg = fill.clone();
        bg_hover = fill.clone();
        bg_active = fill;
        fg = role("interaction-disabled-fg");
        fg_hover = role("interaction-disabled-fg");
        border = role("interaction-disabled-border");
    }
    vec![
        ("bg", bg),
        ("bg_hover", bg_hover),
        ("bg_active", bg_active),
        ("fg", fg),
        ("fg_hover", fg_hover),
        ("border", border),
        ("border_width", border_width),
        ("ring", ring),
        ("ring_width", role("border-width-focus-ring")),
        ("height", role(format!("spacing-{s}-lvl5-height"))),
        ("inset", role(format!("spacing-{s}-lvl5-inset"))),
        ("gap", role(format!("spacing-{s}-content"))),
        ("radius", role(format!("radius-{s}"))),
        (
            "adornment",
            role(format!("spacing-{s}-lvl5-adornment-size")),
        ),
        ("text", role(format!("text-{s}-title1-bold"))),
    ]
}

/// A look field's value, as a token of its type.
fn field(look: &Look, name: &str) -> Token {
    match name {
        "bg" => Token::Color(look.bg),
        "bg_hover" => Token::Color(look.bg_hover),
        "bg_active" => Token::Color(look.bg_active),
        "fg" => Token::Color(look.fg),
        "fg_hover" => Token::Color(look.fg_hover),
        "border" => Token::Color(look.border),
        "ring" => Token::Color(look.ring),
        "border_width" => Token::Number(look.border_width),
        "ring_width" => Token::Number(look.ring_width),
        "height" => Token::Number(look.height),
        "inset" => Token::Number(look.inset),
        "gap" => Token::Number(look.gap),
        "radius" => Token::Number(look.radius),
        "adornment" => Token::Number(look.adornment),
        "text" => Token::Typography(look.text.clone()),
        _ => unreachable!("{name}"),
    }
}

/// Colors compare as `0xRRGGBBAA`.
fn same(a: &Token, b: &Token) -> bool {
    match (a, b) {
        (Token::Color(a), Token::Color(b)) => rgba(*a) == rgba(*b),
        _ => a == b,
    }
}

/// Every field of the look of every case whose value is not its source's, read from `tokens`.
fn mismatches(looks: &[Look], tokens: &Tokens) -> Vec<String> {
    let mut wrong = Vec::new();
    for ((variant, size, outlined, disabled), look) in cases().into_iter().zip(looks) {
        for (name, from) in table(variant, size, outlined, disabled) {
            let expected = match &from {
                From::Role(role) => match tokens.role_by_name(role) {
                    Some(token) => token,
                    None => {
                        wrong.push(format!("{role} resolves to nothing"));
                        continue;
                    }
                },
                From::Transparent => Token::Color(Hsla::default()),
                From::Zero => Token::Number(0.0),
            };
            let got = field(look, name);
            if !same(&got, &expected) {
                wrong.push(format!(
                    "{variant:?} {size:?} outlined={outlined} disabled={disabled}: {name} is {got:?}, {from:?} is {expected:?}"
                ));
            }
        }
    }
    wrong
}

thread_local! {
    static SEEN: RefCell<Option<(Vec<Look>, Arc<Tokens>)>> = const { RefCell::new(None) };
}

/// Every case's look, as Button computes it where it is, and the tokens there.
#[component]
fn Probe() -> impl IntoElement {
    effect(move || {
        let looks: Vec<Look> = cases()
            .into_iter()
            .map(|(v, s, o, d)| Look::of(v, s, o, d))
            .collect();
        let tokens = tokens();
        SEEN.with(|seen| *seen.borrow_mut() = Some((looks, tokens)));
    });
    view()
        .child(button().text("Save").variant(ButtonVariant::Primary))
        .child(text("ready"))
}

fn start_probe(root: Root) -> (Running, Vec<Look>, Arc<Tokens>) {
    SEEN.with(|seen| *seen.borrow_mut() = None);
    let mut app = Running::start(App::Rust(root));
    assert!(app.wait(|s| {
        s.pressable("Save")
            .is_some_and(|id| s.frames.contains_key(&id))
    }));
    let (looks, tokens) = SEEN.with(|seen| seen.borrow().clone()).expect("looks");
    (app, looks, tokens)
}

fn fill(screen: &Screen, label: &str) -> u32 {
    let id = screen.button(label).expect(label);
    let color = screen.styles.get(&id).expect("styled").background_color;
    color.0
}

#[test]
fn under_rayx_theme_every_field_is_its_roles_value() {
    let (_app, looks, tokens) = start_probe(Root::of::<Probe>());
    assert_eq!(
        tokens.document().and_then(|d| d.name.as_deref()),
        Some("rayx")
    );
    assert_eq!(mismatches(&looks, &tokens), Vec::<String>::new());
}

#[test]
fn the_other_theme_has_its_own_tokens_and_a_distinct_value_per_role() {
    let read = nsh_theme::read(OTHER);
    assert_eq!(read.diagnostics, Vec::new());
    let document = read.document;
    let rayx = support::document().0;
    let roles: Vec<&str> = table(ButtonVariant::Neutral, ComponentSize::Default, false, false)
        .into_iter()
        .chain(
            cases()
                .into_iter()
                .flat_map(|(v, s, o, d)| table(v, s, o, d)),
        )
        .filter_map(|(_, from)| match from {
            From::Role(role) => Some(role),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|r| Box::leak(r.into_boxed_str()) as &str)
        .collect();
    assert_eq!(roles.len(), 45, "D-BUTTON's 45 roles: {roles:?}");
    for (name, token) in document.tokens.iter() {
        assert!(
            rayx.tokens.get(name).is_none(),
            "{name} is one of RayX's token names"
        );
        let path = token.path.as_deref().unwrap_or_default();
        assert!(
            rayx.tokens
                .values()
                .all(|t| t.path.as_deref() != Some(path)),
            "{path} is one of RayX's paths"
        );
    }
    for role in &roles {
        let mapped = document
            .roles
            .get(role)
            .unwrap_or_else(|| panic!("{role} is mapped"));
        let token = document.tokens.get(mapped).unwrap();
        let path = token.path.as_deref().unwrap_or_default();
        assert!(
            !path.starts_with("palette."),
            "{role} maps onto the palette's {mapped}"
        );
        let refers = token.value.to_json().to_string().contains('{');
        assert!(refers, "{role}'s {mapped} references the palette");
        assert!(
            document.tokens.get(role).is_none(),
            "no token is named as the role {role}"
        );
    }

    let (tokens, diagnostics) = support::tokens_of(&Arc::new(document), &[]);
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != nsh_theme::code::ROLE_UNMAPPED),
        "{diagnostics:?}"
    );
    let mut by_type: HashMap<String, Vec<(&str, Token)>> = HashMap::new();
    for role in &roles {
        let token = tokens
            .role_by_name(role)
            .unwrap_or_else(|| panic!("{role} resolves"));
        by_type
            .entry(token.value_type().to_string())
            .or_default()
            .push((role, token));
    }
    for (ty, values) in &by_type {
        for (i, (a, x)) in values.iter().enumerate() {
            for (b, y) in &values[i + 1..] {
                assert!(!same(x, y), "{a} and {b} ({ty}) have one value");
            }
            match x {
                Token::Color(c) => assert_eq!(rgba(*c) & 0xff, 0xff, "{a} is opaque"),
                Token::Number(n) => assert!(*n > 0.0, "{a} is no 0"),
                _ => {}
            }
        }
    }
    assert_eq!(by_type["color"].len(), 25);
    assert_eq!(by_type["number"].len(), 17);
    assert_eq!(by_type["typography"].len(), 3);
}

#[test]
fn under_another_theme_every_field_is_exactly_its_roles_value() {
    let (app, looks, tokens) = start_probe(Root::of::<Probe>().theme(OTHER));
    assert_eq!(
        tokens.document().and_then(|d| d.name.as_deref()),
        Some("other")
    );
    assert_eq!(mismatches(&looks, &tokens), Vec::<String>::new());
    // What the mounted primary button draws is the other theme's.
    let save = app.screen.button("Save").unwrap();
    let color = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Color(c)) => rgba(c),
        other => panic!("{role}: {other:?}"),
    };
    let number = |role: &str| match tokens.role_by_name(role) {
        Some(Token::Number(n)) => n,
        other => panic!("{role}: {other:?}"),
    };
    assert_eq!(fill(&app.screen, "Save"), color("intent-primary-bg"));
    assert_eq!(
        app.screen.frames[&save].1,
        number("spacing-default-lvl5-height")
    );
    let style = &app.screen.styles[&save];
    assert_eq!(style.border_radius.0, number("radius-default"));
    let label = app.screen.text_node("Save").unwrap();
    let (font_size, line_height, letter_spacing) = app.screen.text_styles[&label];
    let Some(Token::Typography(Typography {
        size,
        line_height: Some(LineHeight::Points(leading)),
        letter_spacing: Some(tracking),
        ..
    })) = tokens.role_by_name("text-default-title1-bold")
    else {
        panic!("text-default-title1-bold");
    };
    assert_eq!(
        (font_size, line_height, letter_spacing),
        (size.0, leading.0, tracking.0)
    );
}

#[component]
fn Live() -> impl IntoElement {
    view()
        .child(button().text("Save").variant(ButtonVariant::Primary))
        .child(
            button()
                .text("Pick")
                .on_press(|| set_modifier("accent", "sky")),
        )
        .child(button().text("Patch").on_press(|| {
            update_tokens(Patch::new().value(
                "theme-colors-roles-primary-bg",
                Hsla::from(nsh_core::color::rgba(0x123456ff)),
            ));
        }))
        .child(button().text("Theme").on_press(|| {
            set_theme(Some(OTHER));
        }))
        .child(
            button()
                .text("Tone")
                .on_press(|| set_modifier("tone", "vivid")),
        )
}

#[test]
fn a_pick_a_theme_and_a_patch_change_a_mounted_button_in_place() {
    let mut app = Running::start(App::Rust(Root::of::<Live>()));
    assert!(app.wait(|s| {
        s.pressable("Tone")
            .is_some_and(|id| s.frames.contains_key(&id))
    }));
    let save = app.screen.button("Save").unwrap();
    assert_eq!(
        fill(&app.screen, "Save"),
        0x007796ff,
        "cyan's intent-primary-bg, light"
    );

    app.press("Pick");
    assert!(
        app.wait(|s| fill(s, "Save") == 0x075e8eff),
        "sky's intent-primary-bg"
    );
    assert_eq!(app.screen.button("Save"), Some(save), "the same view");

    app.press("Patch");
    assert!(
        app.wait(|s| fill(s, "Save") == 0x123456ff),
        "the patched token"
    );
    assert_eq!(app.screen.button("Save"), Some(save));

    let other = nsh_theme::read(OTHER).document;
    let (tokens, _) = support::tokens_of(&Arc::new(other.clone()), &[]);
    let Some(Token::Color(primary)) = tokens.role_by_name("intent-primary-bg") else {
        panic!("the other theme's primary");
    };
    app.press("Theme");
    assert!(
        app.wait(|s| fill(s, "Save") == rgba(primary)),
        "the other theme's intent-primary-bg"
    );
    assert_eq!(app.screen.button("Save"), Some(save));

    app.press("Tone");
    assert!(
        app.wait(|s| fill(s, "Save") == 0xff00ffff),
        "the other theme's vivid context"
    );
    assert_eq!(app.screen.button("Save"), Some(save));
}

#[component]
fn Ring() -> impl IntoElement {
    view()
        .child(button().text("Save").variant(ButtonVariant::Primary))
        .child(button().text("Widen").on_press(|| {
            update_tokens(Patch::new().value("platform-density-borders-focus-ring-width", 5.0_f32));
        }))
}

#[test]
fn a_patch_of_the_ring_width_token_changes_the_focus_ring_in_place() {
    let mut app = Running::start(App::Rust(Root::of::<Ring>()));
    assert!(app.wait(|s| {
        s.pressable("Widen")
            .is_some_and(|id| s.frames.contains_key(&id))
    }));
    let save = app.screen.button("Save").unwrap();
    // The keyboard focus on the button: `focus-visible` shows its ring.
    let control = app.engine.control();
    control.set_input_modality(nsh_core::platform::InputModality::Keyboard);
    control.set_states(save, nsh_core::tailwind::state::FOCUSED);
    let width = |s: &Screen| s.styles.get(&save).map(|st| st.outline.width.0);
    assert!(
        app.wait(|s| width(s) == Some(2.0)),
        "border-width-focus-ring: {:?}",
        width(&app.screen)
    );

    // A patch of the token the role maps onto: the same view's ring follows.
    let widen = app.screen.pressable("Widen").unwrap();
    app.press_node(widen);
    control.set_states(save, nsh_core::tailwind::state::FOCUSED);
    assert!(
        app.wait(|s| width(s) == Some(5.0)),
        "the patched width: {:?}",
        width(&app.screen)
    );
    assert_eq!(app.screen.button("Save"), Some(save), "the same view");
}

#[test]
fn an_unmapped_role_draws_transparent_or_zero_and_is_reported() {
    let mut json: serde_json::Value = serde_json::from_str(OTHER).unwrap();
    let roles = json["roles"].as_object_mut().unwrap();
    roles.remove("intent-primary-bg").unwrap();
    roles.remove("radius-default").unwrap();
    let text = serde_json::to_string(&json).unwrap();

    let read = nsh_theme::read(&text);
    assert_eq!(read.diagnostics, Vec::new());
    let document = read.document;
    let resolved = nsh_theme::resolve(&document, &nsh_theme::Selection::new(), &[]);
    let declarations = rayx_components::theme::roles::all();
    let unmapped: BTreeSet<String> =
        nsh_theme::check_roles(&document, &[], &resolved, &declarations, &[])
            .into_iter()
            .filter(|d| d.code == nsh_theme::code::ROLE_UNMAPPED)
            .map(|d| d.pointer)
            .collect();
    assert!(
        unmapped.iter().any(|p| p.contains("intent-primary-bg"))
            && unmapped.iter().any(|p| p.contains("radius-default")),
        "{unmapped:?}"
    );

    let (app, looks, _) = start_probe(Root::of::<Probe>().theme(text));
    let index = cases()
        .iter()
        .position(|c| *c == (ButtonVariant::Primary, ComponentSize::Default, false, false))
        .unwrap();
    assert_eq!(rgba(looks[index].bg), 0, "transparent");
    assert_eq!(looks[index].radius, 0.0);
    assert_eq!(fill(&app.screen, "Save"), 0, "drawn transparent");
    let save = app.screen.button("Save").unwrap();
    assert_eq!(app.screen.styles[&save].border_radius.0, 0.0);
}
