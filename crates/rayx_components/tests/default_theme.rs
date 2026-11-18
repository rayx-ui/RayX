//! RayX's default theme, `theme.json` (RayX.Dtcg's export of the design system): NSH reads it
//! with no diagnostic, resolves it under every combination of its contexts, and checks its roles
//! against RayX's, nsh-base's and the core's declarations with no error and no `role-unmapped`;
//! in a headless app whose UI set is `rayx_components::register` it is the active document and
//! every role resolves to a token of its type. Its role mapping is the design's: each RayX role
//! maps onto the token whose DTCG path D-ROLE-NAMES' table names it by, and each of the 33 core
//! and nsh-base roles onto the token it mapped onto before.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use nsh_core::element::description::{SetDescription, theme_document};
use nsh_core::element::{Root, Tokens};
use nsh_core::prelude::*;
use nsh_engine::App;
use nsh_theme::{Severity, code};
use support::Running;

/// The core's and nsh-base's roles RayX's theme maps, and the DTCG path of the token each maps
/// onto (`theme.colors.roles` becomes `theme.colors.intents` in s5).
const NSH_ROLES: [(&str, &str); 33] = [
    ("background", "theme.colors.surfaces.app"),
    ("foreground", "theme.colors.surfaces.text"),
    ("card-foreground", "theme.colors.surfaces.text"),
    ("popover-foreground", "theme.colors.surfaces.text"),
    ("accent-foreground", "theme.colors.surfaces.text"),
    ("sidebar-foreground", "theme.colors.surfaces.text"),
    ("sidebar-accent-foreground", "theme.colors.surfaces.text"),
    ("card", "theme.colors.surfaces.panel1"),
    ("sidebar", "theme.colors.surfaces.panel1"),
    ("popover", "theme.colors.surfaces.overlay"),
    ("muted-foreground", "theme.colors.surfaces.textMuted"),
    ("border", "theme.colors.surfaces.border"),
    ("input", "theme.colors.surfaces.border"),
    ("sidebar-border", "theme.colors.surfaces.border"),
    ("success", "theme.colors.surfaces.textSuccess"),
    ("warning", "theme.colors.surfaces.textWarning"),
    ("info", "theme.colors.surfaces.textInfo"),
    ("primary", "theme.colors.roles.primary.bg"),
    ("sidebar-primary", "theme.colors.roles.primary.bg"),
    ("primary-foreground", "theme.colors.roles.primary.text"),
    (
        "sidebar-primary-foreground",
        "theme.colors.roles.primary.text",
    ),
    ("secondary", "theme.colors.roles.neutral.bg"),
    ("secondary-foreground", "theme.colors.roles.neutral.text"),
    ("muted", "theme.colors.roles.neutral.bgStrong"),
    ("accent", "theme.colors.roles.neutral.bgHover"),
    ("sidebar-accent", "theme.colors.roles.neutral.bgHover"),
    ("destructive", "theme.colors.roles.danger.bg"),
    ("ring", "theme.colors.interaction.focusRing"),
    ("sidebar-ring", "theme.colors.interaction.focusRing"),
    ("selection", "theme.colors.interaction.textSelectionBg"),
    ("radius", "platform.density.size.default.borders.radius"),
    ("font-sans", "platform.density.typography.fontFamilyPrimary"),
    ("font-mono", "platform.density.typography.fontFamilyMono"),
];

const SIZES: [&str; 3] = ["small", "default", "large"];

/// A path segment in kebab case: `bgHover` is `bg-hover`, `panel1Hover` `panel1-hover`.
fn kebab(segment: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in segment.chars() {
        if c.is_ascii_uppercase()
            && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
        previous = Some(c);
    }
    out
}

/// D-ROLE-NAMES' table: the role a token of this DTCG path plays, `None` for a token that is no
/// role.
fn role_of_path(path: &str) -> Option<String> {
    let p: Vec<&str> = path.split('.').collect();
    let k: Vec<String> = p.iter().map(|s| kebab(s)).collect();
    let at = |prefix: &[&str]| p.len() >= prefix.len() && p[..prefix.len()] == *prefix;
    if at(&["theme", "colors", "surfaces"]) && p.len() == 4 {
        return Some(format!("surface-{}", k[3]));
    }
    if at(&["theme", "colors", "roles"]) || at(&["theme", "colors", "intents"]) {
        return match p.len() {
            5 => Some(format!("intent-{}-{}", k[3], k[4])),
            6 if p[3] == "alerts" => Some(format!("intent-alerts-{}-{}", k[4], k[5])),
            _ => None,
        };
    }
    if at(&["theme", "colors", "interaction"]) && p.len() == 4 {
        return Some(format!("interaction-{}", k[3]));
    }
    if at(&["platform", "density", "size"]) && p.len() == 6 && SIZES.contains(&p[3]) {
        let size = p[3];
        if let Some(level) = p[4].strip_prefix("lvl") {
            return Some(format!("spacing-{size}-lvl{level}-{}", k[5]));
        }
        return match (p[4], p[5]) {
            ("borders", "radius") => Some(format!("radius-{size}")),
            ("borders", "radiusInner") => Some(format!("radius-{size}-inner")),
            ("spacing", _) => Some(format!("spacing-{size}-{}", k[5])),
            _ => None,
        };
    }
    if at(&["platform", "density", "borders"]) && p.len() == 4 {
        let name = p[3];
        if name == "radiusFull" {
            return Some("radius-full".into());
        }
        if let Some(n) = name.strip_prefix("radius") {
            return Some(format!("radius-{n}"));
        }
        if let Some(n) = name.strip_suffix("Width") {
            return Some(format!("border-width-{}", kebab(n)));
        }
        return None;
    }
    if (at(&["platform", "density", "spacing"]) || at(&["platform", "density", "insets"]))
        && p.len() == 4
    {
        return Some(format!("spacing-{}", p[3]));
    }
    match path {
        "platform.density.interaction.minTarget" => return Some("spacing-min-target".into()),
        "platform.density.typography.fontFamilyPrimary" => return Some("font-primary".into()),
        "platform.density.typography.fontFamilyMono" => return Some("font-mono".into()),
        _ => {}
    }
    if at(&["theme", "styles", "text", "size"]) && p.len() == 6 {
        return Some(format!("text-{}-{}", p[4], k[5]));
    }
    if at(&["theme", "styles", "text", "typography"]) && p.len() == 5 {
        return Some(format!("text-{}", k[4]));
    }
    if at(&["theme", "styles", "shadow"]) && p.len() == 4 {
        return Some(format!("shadow-{}", k[3]));
    }
    if at(&["theme", "styles", "focusRing"]) && p.len() == 4 {
        return Some(format!("shadow-focus-ring-{}", k[3]));
    }
    if at(&["theme", "styles", "blur"]) && p.len() == 4 {
        return Some(format!("blur-{}", k[3]));
    }
    if at(&["theme", "styles", "bgBlur"]) && p.len() == 4 {
        return Some(format!("blur-bg-{}", k[3]));
    }
    None
}

/// RayX's roles, with their value types, from `theme::roles::all()`.
fn rayx_roles() -> Vec<(String, String)> {
    rayx_components::theme::roles::all()
        .iter()
        .map(|r| (r.name.clone(), r.value_type.to_string()))
        .collect()
}

/// Every combination of the document's modifiers' contexts.
fn combinations(document: &nsh_theme::ThemeDocument) -> Vec<Vec<(String, String)>> {
    let mut all = vec![Vec::new()];
    for (name, modifier) in document.modifiers.iter() {
        let mut next = Vec::new();
        for picks in &all {
            for (context, _) in modifier.contexts.iter() {
                let mut picks: Vec<(String, String)> = picks.clone();
                picks.push((name.to_owned(), context.to_owned()));
                next.push(picks);
            }
        }
        all = next;
    }
    all
}

#[test]
fn nsh_reads_it_with_no_diagnostic() {
    let (document, diagnostics) = support::document();
    assert_eq!(diagnostics, Vec::new());
    assert_eq!(document.name.as_deref(), Some("rayx"));
    let modifiers: Vec<&str> = document.modifiers.iter().map(|(n, _)| n).collect();
    assert_eq!(modifiers, ["colorMode", "accent", "density", "os"]);
    // Tokens are named by their DTCG paths in kebab case.
    let primary = document
        .tokens
        .get("theme-colors-roles-primary-bg")
        .expect("token");
    assert_eq!(
        primary.path.as_deref(),
        Some("theme.colors.roles.primary.bg")
    );
}

#[test]
fn it_resolves_and_checks_its_roles_under_every_combination() {
    let (document, _) = support::document();
    let all = combinations(&document);
    assert_eq!(all.len(), 2 * 3 * 2 * 4);
    for picks in all {
        let picks: Vec<(&str, &str)> = picks
            .iter()
            .map(|(m, c)| (m.as_str(), c.as_str()))
            .collect();
        let (_, diagnostics) = support::tokens_of(&document, &picks);
        let wrong: Vec<String> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error || d.code == code::ROLE_UNMAPPED)
            .map(ToString::to_string)
            .collect();
        assert_eq!(wrong, Vec::<String>::new(), "{picks:?}");
    }
}

#[test]
fn every_rayx_role_maps_onto_the_token_its_name_comes_from() {
    let (document, _) = support::document();
    let mut wrong = Vec::new();
    for (role, _) in rayx_roles() {
        let path = document
            .roles
            .get(&role)
            .and_then(|token| document.tokens.get(token))
            .and_then(|token| token.path.clone());
        match path {
            Some(path) if role_of_path(&path).as_deref() == Some(role.as_str()) => {}
            other => wrong.push(format!("{role} -> {other:?}")),
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn the_core_and_nsh_base_roles_map_onto_the_tokens_they_mapped_onto() {
    let (document, _) = support::document();
    for (role, path) in NSH_ROLES {
        let token = document
            .roles
            .get(role)
            .unwrap_or_else(|| panic!("{role} is mapped"));
        assert_eq!(
            document.tokens.get(token).and_then(|t| t.path.as_deref()),
            Some(path),
            "{role}"
        );
    }
    // The document maps nothing else.
    assert_eq!(
        document.roles.len(),
        rayx_roles().len() + NSH_ROLES.len() - 1
    );
}

thread_local! {
    static TOKENS: RefCell<Option<Arc<Tokens>>> = const { RefCell::new(None) };
}

#[component]
fn Reader() -> impl IntoElement {
    effect(move || {
        let tokens = tokens();
        TOKENS.with(|t| *t.borrow_mut() = Some(tokens));
    });
    text("ready")
}

#[test]
fn in_an_app_it_is_the_active_document_and_every_role_resolves_to_its_type() {
    let sets: Vec<SetDescription> = support::ui_sets()
        .iter()
        .map(SetDescription::from)
        .collect();
    let (active, diagnostics) = theme_document(&sets);
    let (document, _) = support::document();
    assert_eq!(active.as_ref(), Some(&*document));
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&code::LEGACY_FORMAT) && !codes.contains(&code::DOCUMENT_UNUSED),
        "{diagnostics:?}"
    );

    let mut app = Running::start(App::Rust(Root::of::<Reader>()));
    assert!(app.wait(|s| s.text_node("ready").is_some()));
    let tokens = TOKENS.with(|t| t.borrow().clone()).expect("tokens read");
    assert_eq!(
        tokens.document(),
        Some(&*document),
        "RayX's document is active"
    );

    let kinds: HashMap<String, String> = document
        .tokens
        .iter()
        .map(|(name, token)| (name.to_owned(), token.value_type.to_string()))
        .collect();
    let mut wrong = Vec::new();
    for (role, ty) in rayx_roles() {
        match tokens.role_by_name(&role) {
            Some(token) if token.value_type().to_string() == ty => {}
            other => wrong.push(format!("{role} ({ty}): {other:?}")),
        }
    }
    for (role, _) in NSH_ROLES {
        let ty = &kinds[document.roles.get(role).expect(role)];
        match tokens.role_by_name(role) {
            Some(token) if token.value_type().to_string() == *ty => {}
            other => wrong.push(format!("{role} ({ty}): {other:?}")),
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}
