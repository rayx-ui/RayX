//! RayX's role list (`roles.json`) and its typed handles: every entry is one role of the design
//! system's semantic layer, named and typed by D-ROLE-NAMES' table, the required ones are
//! exactly Button's 45, `theme::roles::all()` is the file entry for entry, the typed groups name
//! declared roles of their types, and `register` lists them all in the UI set, so NSH's
//! `roles.d.ts` types TS's `role()` with them.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::collections::{BTreeSet, HashMap};

use nsh_core::element::description::SetDescription;
use nsh_core::element::{Registry, RoleSpec};
use rayx_components::theme::{ComponentSize, IntentRoles, roles};

const ROLES: &str = include_str!("../roles.json");

/// Button's 45 roles (D-BUTTON), the roles a RayX component reads: RayX's required roles.
const BUTTON: [&str; 45] = [
    "intent-neutral-bg",
    "intent-neutral-bg-hover",
    "intent-neutral-bg-active",
    "intent-neutral-text",
    "intent-primary-bg",
    "intent-primary-bg-hover",
    "intent-primary-bg-active",
    "intent-primary-text",
    "intent-danger-bg",
    "intent-danger-bg-hover",
    "intent-danger-bg-active",
    "intent-danger-text",
    "intent-success-bg",
    "intent-success-bg-hover",
    "intent-success-bg-active",
    "intent-success-text",
    "surface-link",
    "surface-link-hover",
    "surface-border",
    "interaction-focus-ring",
    "interaction-focus-ring-danger",
    "interaction-focus-ring-success",
    "interaction-disabled-border",
    "interaction-disabled-bg",
    "interaction-disabled-fg",
    "border-width-outline",
    "border-width-focus-ring",
    "spacing-small-lvl5-height",
    "spacing-small-lvl5-inset",
    "spacing-small-lvl5-adornment-size",
    "spacing-small-content",
    "radius-small",
    "text-small-title1-bold",
    "spacing-default-lvl5-height",
    "spacing-default-lvl5-inset",
    "spacing-default-lvl5-adornment-size",
    "spacing-default-content",
    "radius-default",
    "text-default-title1-bold",
    "spacing-large-lvl5-height",
    "spacing-large-lvl5-inset",
    "spacing-large-lvl5-adornment-size",
    "spacing-large-content",
    "radius-large",
    "text-large-title1-bold",
];

const TYPES: [&str; 13] = [
    "color",
    "number",
    "font",
    "string",
    "boolean",
    "duration",
    "easing",
    "typography",
    "shadow",
    "gradient",
    "blur",
    "border",
    "transition",
];

const SIZES: [&str; 3] = ["small", "default", "large"];

/// One or more kebab-case segments (`text-muted`, `panel1-hover`, `xxl`).
fn kebab(rest: &str) -> bool {
    !rest.is_empty()
        && rest.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn digits(rest: &str) -> bool {
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// The value type D-ROLE-NAMES' table gives a role of this name; `None` for a name outside it.
fn pattern_type(name: &str) -> Option<&'static str> {
    let after = |prefix: &str| name.strip_prefix(prefix);
    for size in SIZES {
        if let Some(rest) = after(&format!("spacing-{size}-lvl")) {
            let (level, part) = rest.split_once('-')?;
            let level_ok = matches!(level, "1" | "2" | "3" | "4" | "5" | "6" | "7");
            return (level_ok && kebab(part)).then_some("number");
        }
        if name == format!("radius-{size}") || name == format!("radius-{size}-inner") {
            return Some("number");
        }
        if let Some(rest) = after(&format!("spacing-{size}-")) {
            return kebab(rest).then_some("number");
        }
        if let Some(rest) = after(&format!("text-{size}-")) {
            return kebab(rest).then_some("typography");
        }
    }
    if let Some(rest) = after("surface-") {
        return kebab(rest).then_some("color");
    }
    if let Some(rest) = after("intent-alerts-") {
        let (kind, part) = rest.split_once('-')?;
        return (kebab(kind) && kebab(part)).then_some("color");
    }
    if let Some(rest) = after("intent-") {
        let (intent, part) = rest.split_once('-')?;
        return (kebab(intent) && kebab(part)).then_some("color");
    }
    if let Some(rest) = after("interaction-") {
        return kebab(rest).then_some("color");
    }
    if let Some(rest) = after("radius-") {
        return (digits(rest) || rest == "full").then_some("number");
    }
    if let Some(rest) = after("border-width-") {
        return kebab(rest).then_some("number");
    }
    if let Some(rest) = after("spacing-gap").or_else(|| after("spacing-inset")) {
        return digits(rest).then_some("number");
    }
    if name == "spacing-min-target" {
        return Some("number");
    }
    if name == "font-primary" || name == "font-mono" {
        return Some("font");
    }
    if let Some(rest) = after("text-") {
        return kebab(rest).then_some("typography");
    }
    if let Some(rest) = after("shadow-") {
        return kebab(rest).then_some("shadow");
    }
    if let Some(rest) = after("blur-") {
        return kebab(rest).then_some("blur");
    }
    None
}

/// One entry of `roles.json`.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    name: String,
    ty: String,
    doc: Option<String>,
    required: bool,
}

fn entries() -> Vec<Entry> {
    let list: serde_json::Value = serde_json::from_str(ROLES).expect("roles.json is JSON");
    list.as_array()
        .expect("roles.json is an array")
        .iter()
        .map(|e| Entry {
            name: e["name"].as_str().unwrap_or_default().to_owned(),
            ty: e["type"].as_str().unwrap_or_default().to_owned(),
            doc: e.get("doc").and_then(|d| d.as_str()).map(str::to_owned),
            required: e.get("required").and_then(|r| r.as_bool()).unwrap_or(false),
        })
        .collect()
}

fn entry_of(spec: &RoleSpec) -> Entry {
    Entry {
        name: spec.name.clone(),
        ty: spec.value_type.to_string(),
        doc: spec.doc.clone(),
        required: spec.required,
    }
}

#[test]
fn every_entry_is_a_role_of_the_semantic_layer_once() {
    let mut seen = BTreeSet::new();
    let mut wrong = Vec::new();
    for entry in entries() {
        if !seen.insert(entry.name.clone()) {
            wrong.push(format!("`{}` is listed twice", entry.name));
        }
        if !TYPES.contains(&entry.ty.as_str()) {
            wrong.push(format!(
                "`{}` has the unknown type `{}`",
                entry.name, entry.ty
            ));
        }
        match pattern_type(&entry.name) {
            None => wrong.push(format!(
                "`{}` is outside D-ROLE-NAMES' patterns",
                entry.name
            )),
            Some(ty) if ty != entry.ty => wrong.push(format!(
                "`{}` is a {}, and its pattern's roles are {ty}s",
                entry.name, entry.ty
            )),
            Some(_) => {}
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
    assert!(seen.len() > 300, "the whole semantic layer: {}", seen.len());
}

#[test]
fn the_required_roles_are_buttons() {
    let required: BTreeSet<String> = entries()
        .into_iter()
        .filter(|e| e.required)
        .map(|e| e.name)
        .collect();
    let button: BTreeSet<String> = BUTTON.iter().map(|r| (*r).to_owned()).collect();
    assert_eq!(button.len(), 45);
    let missing: Vec<_> = button.difference(&required).collect();
    let extra: Vec<_> = required.difference(&button).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "not required: {missing:?}; required but not Button's: {extra:?}"
    );
}

#[test]
fn the_patterns_refuse_names_outside_the_table() {
    for name in [
        "primary",
        "theme-colors-roles-primary-bg",
        "spacing-default-lvl8-height",
        "radius-medium",
        "intent-primary",
        "font-sans",
        "text-",
        "palette-red-500",
    ] {
        assert_eq!(pattern_type(name), None, "{name}");
    }
    assert_eq!(pattern_type("spacing-default-lvl5-height"), Some("number"));
    assert_eq!(pattern_type("intent-alerts-info-bg"), Some("color"));
    assert_eq!(pattern_type("text-default-title1-bold"), Some("typography"));
    assert_eq!(pattern_type("text-h1"), Some("typography"));
    assert_eq!(pattern_type("blur-bg-md"), Some("blur"));
}

#[test]
fn all_is_the_file_entry_for_entry() {
    let generated: Vec<Entry> = roles::all().iter().map(entry_of).collect();
    let file = entries();
    assert_eq!(generated.len(), file.len());
    for (generated, file) in generated.iter().zip(&file) {
        assert_eq!(generated, file);
    }
    // The constants are the roles, typed by their value types.
    assert_eq!(
        roles::INTENT_PRIMARY_BG_HOVER.name(),
        "intent-primary-bg-hover"
    );
    assert_eq!(
        roles::SPACING_DEFAULT_LVL5_HEIGHT.name(),
        "spacing-default-lvl5-height"
    );
    assert_eq!(
        roles::TEXT_DEFAULT_TITLE1_BOLD.name(),
        "text-default-title1-bold"
    );
    assert_eq!(roles::FONT_PRIMARY.name(), "font-primary");
    assert_eq!(roles::SHADOW_MD.name(), "shadow-md");
    assert_eq!(roles::BLUR_BG_MD.name(), "blur-bg-md");
}

#[test]
fn the_typed_groups_name_declared_roles_of_their_types() {
    let declared: HashMap<String, String> = entries().into_iter().map(|e| (e.name, e.ty)).collect();
    let mut wrong = Vec::new();
    let mut check = |name: &str, ty: &str| {
        if declared.get(name).map(String::as_str) != Some(ty) {
            wrong.push(format!("{name} ({ty})"));
        }
    };
    for intent in [
        IntentRoles::NEUTRAL,
        IntentRoles::PRIMARY,
        IntentRoles::DANGER,
        IntentRoles::SUCCESS,
        IntentRoles::ALERT_INFO,
        IntentRoles::ALERT_WARNING,
        IntentRoles::ALERT_ERROR,
        IntentRoles::ALERT_SUCCESS,
    ] {
        for role in [
            intent.bg,
            intent.bg_hover,
            intent.bg_active,
            intent.bg_selected,
            intent.bg_selected_hover,
            intent.bg_translucent,
            intent.text,
            intent.text_muted,
            intent.text_subtle,
            intent.accent,
        ] {
            check(role.name(), "color");
        }
    }
    for size in ComponentSize::ALL {
        let sizes = size.roles();
        for n in 1..=7 {
            let level = sizes.level(n);
            for role in [
                level.height,
                level.inner_height,
                level.inner_inset,
                level.inset,
                level.adornment_size,
            ] {
                check(role.name(), "number");
            }
            assert!(level.height.name().contains(&format!("-lvl{n}-")));
        }
        for role in [sizes.radius, sizes.radius_inner, sizes.content, sizes.items] {
            check(role.name(), "number");
        }
        let text = sizes.text;
        for role in [
            text.regular,
            text.code,
            text.subtitle1_bold,
            text.subtitle1_regular,
            text.title1_bold,
            text.title1_regular,
            text.title2_regular,
            text.title3_regular,
        ] {
            check(role.name(), "typography");
        }
        assert!(sizes.radius.name().ends_with(size.name()));
    }
    assert_eq!(wrong, Vec::<String>::new());
    // Levels clamp to 1..=7.
    let sizes = ComponentSize::Default.roles();
    assert_eq!(sizes.level(0), sizes.level(1));
    assert_eq!(sizes.level(9), sizes.level(7));
}

/// The groups `build.rs` generates: every field is a constant of `theme::roles`, never a role
/// made from a string, so a role that leaves `roles.json` fails to compile in them (the
/// `compile_fail` doc test on `IntentRoles` shows a missing constant failing).
#[test]
fn the_typed_groups_are_built_over_the_constants() {
    let declared: BTreeSet<String> = entries()
        .into_iter()
        .map(|e| e.name.replace('-', "_").to_ascii_uppercase())
        .collect();
    for source in [
        include_str!(concat!(env!("OUT_DIR"), "/intent_groups.rs")),
        include_str!(concat!(env!("OUT_DIR"), "/size_groups.rs")),
    ] {
        assert!(!source.contains("::new("), "{source}");
        let fields: Vec<&str> = source
            .lines()
            .filter_map(|line| line.trim().split_once(": roles::"))
            .map(|(_, constant)| constant.trim_end_matches(','))
            .collect();
        assert!(!fields.is_empty());
        for constant in fields {
            assert!(
                declared.contains(constant),
                "{constant} is no role of roles.json"
            );
        }
    }
    assert_eq!(
        IntentRoles::PRIMARY.bg_hover,
        roles::INTENT_PRIMARY_BG_HOVER
    );
    assert_eq!(
        IntentRoles::ALERT_INFO.accent,
        roles::INTENT_ALERTS_INFO_ACCENT
    );
    let large = ComponentSize::Large.roles();
    assert_eq!(
        large.level(5).adornment_size,
        roles::SPACING_LARGE_LVL5_ADORNMENT_SIZE
    );
    assert_eq!(large.radius_inner, roles::RADIUS_LARGE_INNER);
    assert_eq!(large.text.title1_bold, roles::TEXT_LARGE_TITLE1_BOLD);
}

#[test]
fn register_lists_every_role_in_the_ui_set() {
    let mut set = Registry::ui_set("test/native");
    rayx_components::register(&mut set);
    let listed: HashMap<String, Entry> = set
        .role_specs()
        .iter()
        .map(|s| (s.name.clone(), entry_of(s)))
        .collect();
    let file = entries();
    for entry in &file {
        assert_eq!(listed.get(&entry.name), Some(entry), "{}", entry.name);
    }
    // Merged with nsh-base's set (first, as a host orders them), every RayX role keeps its
    // declaration. A role nsh-base declares too (`font-mono`) is one role of one type, with
    // nsh-base's declaration, which comes first.
    let base: BTreeSet<String> = nsh_base::registry()
        .role_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let merged: HashMap<String, Entry> = support::declarations()
        .iter()
        .map(|s| (s.name.clone(), entry_of(s)))
        .collect();
    for entry in &file {
        let found = merged.get(&entry.name).expect(&entry.name);
        if base.contains(&entry.name) {
            assert_eq!((&found.name, &found.ty), (&entry.name, &entry.ty));
        } else {
            assert_eq!(found, entry);
        }
    }
    assert!(base.contains("font-mono") && merged.contains_key("primary"));
}

#[test]
fn roles_d_ts_types_role_with_every_rayx_role() {
    let sets: Vec<SetDescription> = support::ui_sets()
        .iter()
        .map(SetDescription::from)
        .collect();
    let dts = nsh_bundler::roles_declaration(&sets).expect("no role two sets type differently");
    let mut missing = Vec::new();
    for entry in entries() {
        let member = format!("{:?}:", entry.name);
        if !dts.contains(&member) {
            missing.push(entry.name);
        }
    }
    assert_eq!(missing, Vec::<String>::new(), "{dts}");
    assert!(dts.contains("Required: every theme maps it."));
}
