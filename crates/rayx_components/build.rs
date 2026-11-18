//! Generates `theme::roles` from `roles.json`, RayX's role list: one typed `pub const` per role
//! (named as the role in upper snake case, documented with its doc, typed by its value type) and
//! `all()`, the declarations a UI set registers (`Registry::roles`). A malformed entry, a
//! duplicate name or an unknown type fails the build, naming the entry.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::PathBuf;

/// NSH's value types and the `Role` each is read through.
const TYPES: &[(&str, &str)] = &[
    ("color", "Role"),
    ("number", "NumberRole"),
    ("font", "FontRole"),
    ("string", "Role<String>"),
    ("boolean", "Role<bool>"),
    ("duration", "Role<::std::time::Duration>"),
    ("easing", "Role<CubicBezier>"),
    ("typography", "Role<Typography>"),
    ("shadow", "Role<Arc<[BoxShadow]>>"),
    ("gradient", "Role<Gradient>"),
    ("blur", "Role<Blur>"),
    ("border", "Role<Border>"),
    ("transition", "Role<TransitionToken>"),
];

fn fail(message: String) -> ! {
    panic!("roles.json: {message}");
}

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let path = manifest.join("roles.json");
    println!("cargo:rerun-if-changed=roles.json");
    println!("cargo:rerun-if-changed=build.rs");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    let entries: Vec<serde_json::Value> = match serde_json::from_str(&text) {
        Ok(serde_json::Value::Array(entries)) => entries,
        Ok(_) => fail("the role list is a JSON array of { name, type, doc?, required? }".into()),
        Err(e) => fail(format!("not JSON: {e}")),
    };

    let mut seen = HashSet::new();
    let mut constants = String::new();
    let mut all = String::new();
    for (i, entry) in entries.iter().enumerate() {
        let Some(object) = entry.as_object() else {
            fail(format!("entry {i} is no object"));
        };
        let name = match object.get("name") {
            Some(serde_json::Value::String(name)) => name.as_str(),
            _ => fail(format!("entry {i} has no `name` string")),
        };
        let at = format!("entry {i} (`{name}`)");
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || !name.starts_with(|c: char| c.is_ascii_lowercase())
            || name.ends_with('-')
            || name.contains("--")
        {
            fail(format!(
                "{at}: a role's name is kebab case (`intent-primary-bg`)"
            ));
        }
        if !seen.insert(name) {
            fail(format!("{at}: the name is listed twice"));
        }
        for member in object.keys() {
            if !["name", "type", "doc", "required"].contains(&member.as_str()) {
                fail(format!("{at}: unknown member `{member}`"));
            }
        }
        let ty = match object.get("type") {
            Some(serde_json::Value::String(ty)) => ty.as_str(),
            _ => fail(format!("{at}: no `type` string")),
        };
        let Some((_, rust)) = TYPES.iter().find(|(t, _)| *t == ty) else {
            fail(format!("{at}: unknown type `{ty}`"));
        };
        let doc = match object.get("doc") {
            None => None,
            Some(serde_json::Value::String(doc)) => Some(doc.as_str()),
            Some(_) => fail(format!("{at}: `doc` is a string")),
        };
        let required = match object.get("required") {
            None => false,
            Some(serde_json::Value::Bool(required)) => *required,
            Some(_) => fail(format!("{at}: `required` is true or false")),
        };
        let constant = name.replace('-', "_").to_ascii_uppercase();
        match doc {
            Some(doc) => {
                for line in doc.lines() {
                    let _ = writeln!(constants, "#[doc = {:?}]", line);
                }
            }
            None => {
                let _ = writeln!(constants, "#[doc = {:?}]", format!("Name `{name}`."));
            }
        }
        let _ = writeln!(constants, "#[doc = \"\"]");
        let _ = writeln!(
            constants,
            "#[doc = {:?}]",
            format!(
                "Role `{name}`, a {ty}{}.",
                if required { ", required" } else { "" }
            )
        );
        let _ = writeln!(
            constants,
            "pub const {constant}: {rust} = <{rust}>::new({name:?});"
        );
        let _ = writeln!(
            all,
            "        {constant}.declaration({:?}, {required}),",
            doc.unwrap_or("")
        );
    }

    let out = format!(
        "{constants}\n/// RayX's roles' declarations, with their docs and whether each is required, as \
         `crate::register` registers them (`Registry::roles`).\npub fn all() -> Vec<RoleSpec> {{\n    vec![\n{all}    ]\n}}\n"
    );
    let dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    let write = |file: &str, text: String| {
        std::fs::write(dir.join(file), text)
            .unwrap_or_else(|e| fail(format!("writing {file}: {e}")));
    };
    write("roles.rs", out);
    write("intent_groups.rs", intent_groups());
    write("size_groups.rs", size_groups());
}

/// A role's constant in `theme::roles`.
fn constant(role: &str) -> String {
    format!("roles::{}", role.replace('-', "_").to_ascii_uppercase())
}

/// The typed intent groups, each field the constant of its role: a role that leaves `roles.json`
/// fails to compile here.
fn intent_groups() -> String {
    const FIELDS: [(&str, &str); 10] = [
        ("bg", "bg"),
        ("bg_hover", "bg-hover"),
        ("bg_active", "bg-active"),
        ("bg_selected", "bg-selected"),
        ("bg_selected_hover", "bg-selected-hover"),
        ("bg_translucent", "bg-translucent"),
        ("text", "text"),
        ("text_muted", "text-muted"),
        ("text_subtle", "text-subtle"),
        ("accent", "accent"),
    ];
    const INTENTS: [(&str, &str, &str); 8] = [
        (
            "NEUTRAL",
            "neutral",
            "The neutral intent (its `bg-strong` is [`roles::INTENT_NEUTRAL_BG_STRONG`]).",
        ),
        (
            "PRIMARY",
            "primary",
            "The primary intent, the accent's hue.",
        ),
        ("DANGER", "danger", "The danger intent."),
        ("SUCCESS", "success", "The success intent."),
        ("ALERT_INFO", "alerts-info", "The info alert's intent."),
        (
            "ALERT_WARNING",
            "alerts-warning",
            "The warning alert's intent.",
        ),
        ("ALERT_ERROR", "alerts-error", "The error alert's intent."),
        (
            "ALERT_SUCCESS",
            "alerts-success",
            "The success alert's intent.",
        ),
    ];
    let mut out = String::from("impl IntentRoles {\n");
    for (name, intent, doc) in INTENTS {
        let _ = writeln!(
            out,
            "    #[doc = {doc:?}]\n    pub const {name}: IntentRoles = IntentRoles {{"
        );
        for (field, part) in FIELDS {
            let _ = writeln!(
                out,
                "        {field}: {},",
                constant(&format!("intent-{intent}-{part}"))
            );
        }
        out.push_str("    };\n");
    }
    out.push_str("}\n");
    out
}

/// The typed size groups (`SMALL`, `DEFAULT`, `LARGE`), each field the constant of its role.
fn size_groups() -> String {
    const LEVEL: [(&str, &str); 5] = [
        ("height", "height"),
        ("inner_height", "inner-height"),
        ("inner_inset", "inner-inset"),
        ("inset", "inset"),
        ("adornment_size", "adornment-size"),
    ];
    const TEXT: [(&str, &str); 8] = [
        ("regular", "regular"),
        ("code", "code"),
        ("subtitle1_bold", "subtitle1-bold"),
        ("subtitle1_regular", "subtitle1-regular"),
        ("title1_bold", "title1-bold"),
        ("title1_regular", "title1-regular"),
        ("title2_regular", "title2-regular"),
        ("title3_regular", "title3-regular"),
    ];
    let mut out = String::new();
    for (name, size) in [
        ("SMALL", "small"),
        ("DEFAULT", "default"),
        ("LARGE", "large"),
    ] {
        let _ = writeln!(
            out,
            "static {name}: SizeRoles = SizeRoles {{\n    levels: ["
        );
        for n in 1..=7 {
            out.push_str("        LevelRoles {\n");
            for (field, part) in LEVEL {
                let _ = writeln!(
                    out,
                    "            {field}: {},",
                    constant(&format!("spacing-{size}-lvl{n}-{part}"))
                );
            }
            out.push_str("        },\n");
        }
        out.push_str("    ],\n");
        let _ = writeln!(out, "    radius: {},", constant(&format!("radius-{size}")));
        let _ = writeln!(
            out,
            "    radius_inner: {},",
            constant(&format!("radius-{size}-inner"))
        );
        let _ = writeln!(
            out,
            "    content: {},",
            constant(&format!("spacing-{size}-content"))
        );
        let _ = writeln!(
            out,
            "    items: {},",
            constant(&format!("spacing-{size}-items"))
        );
        out.push_str("    text: SizeTextRoles {\n");
        for (field, style) in TEXT {
            let _ = writeln!(
                out,
                "        {field}: {},",
                constant(&format!("text-{size}-{style}"))
            );
        }
        out.push_str("    },\n};\n");
    }
    out
}
