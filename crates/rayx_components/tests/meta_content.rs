//! What RayX's own meta files say: each of the library's components describes itself and groups
//! its states in sections, in the order a reader goes through them. The file shapes
//! (`meta_files`) and the types (`meta_sections`) are checked elsewhere; this group reads the
//! three files' content: a `description` and `sections` with no top-level `states`, a title and
//! a description on every section, Button's, Slot's and AutoLayout's sections and state names as
//! the owner asked for them, every name of the baseline (`fixtures/meta-states.json`, written
//! before the files were rewritten into sections and never changed) kept in its entry of its file,
//! and floors a placeholder cannot pass: no empty description, none equal to its section's title
//! or to what a generator writes, no section called "States" or "Default", no two files with the
//! same description.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::ast::ast::{Expression, ObjectExpression, ObjectPropertyKind, PropertyKey, Statement};
use oxc::parser::Parser;
use oxc::span::SourceType;

/// One section as written: its title, its description and its states' names in order.
#[derive(Debug)]
struct Section {
    title: String,
    description: String,
    states: Vec<String>,
}

/// A meta file's content: its entry's id, its description, whether it has a top-level `states`
/// and its sections.
#[derive(Debug)]
struct Content {
    id: String,
    description: String,
    has_states: bool,
    sections: Vec<Section>,
}

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn key_name(key: &PropertyKey) -> String {
    match key {
        PropertyKey::StaticIdentifier(i) => i.name.to_string(),
        PropertyKey::StringLiteral(s) => s.value.to_string(),
        other => panic!("a computed key: {other:?}"),
    }
}

fn properties<'a, 'b>(object: &'b ObjectExpression<'a>) -> Vec<(String, &'b Expression<'a>)> {
    object
        .properties
        .iter()
        .map(|property| match property {
            ObjectPropertyKind::ObjectProperty(p) => (key_name(&p.key), &p.value),
            other => panic!("a spread: {other:?}"),
        })
        .collect()
}

fn string(value: &Expression) -> String {
    match value.get_inner_expression() {
        Expression::StringLiteral(s) => s.value.to_string(),
        Expression::TemplateLiteral(t) => t.single_quasi().unwrap().to_string(),
        other => panic!("not a string literal: {other:?}"),
    }
}

fn object<'a, 'b>(value: &'b Expression<'a>) -> &'b ObjectExpression<'a> {
    match value.get_inner_expression() {
        Expression::ObjectExpression(o) => o,
        other => panic!("not an object literal: {other:?}"),
    }
}

fn field<'a, 'b>(
    fields: &[(String, &'b Expression<'a>)],
    name: &str,
) -> Option<&'b Expression<'a>> {
    fields.iter().find(|(key, _)| key == name).map(|(_, v)| *v)
}

/// Reads `src/<file>` as a tool does.
fn read(file: &str) -> Content {
    let text = std::fs::read_to_string(crate_dir().join(file)).unwrap();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &text, SourceType::tsx()).parse();
    assert!(parsed.diagnostics.errors().next().is_none(), "{file}");
    let exported = parsed
        .program
        .body
        .iter()
        .find_map(|statement| match statement {
            Statement::ExportDefaultDeclaration(export) => export.declaration.as_expression(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{file} has no default export"));
    let fields = properties(object(exported));
    let id = string(field(&fields, "id").unwrap_or_else(|| panic!("{file} has no id")));
    let description = field(&fields, "description")
        .map(string)
        .unwrap_or_else(|| panic!("{file} has no description"));
    let has_states = field(&fields, "states").is_some();
    let Some(Expression::ArrayExpression(list)) =
        field(&fields, "sections").map(Expression::get_inner_expression)
    else {
        panic!("{file} has no `sections` array");
    };
    let sections = list
        .elements
        .iter()
        .map(|element| {
            let section = properties(object(element.as_expression().unwrap()));
            let title = field(&section, "title")
                .map(string)
                .unwrap_or_else(|| panic!("{file}: a section has no title"));
            let description = field(&section, "description")
                .map(string)
                .unwrap_or_else(|| panic!("{file}: the section `{title}` has no description"));
            let states = properties(object(field(&section, "states").unwrap()))
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            Section {
                title,
                description,
                states,
            }
        })
        .collect();
    Content {
        id,
        description,
        has_states,
        sections,
    }
}

/// RayX's meta files, by their path under the crate.
const FILES: [&str; 3] = [
    "src/button.meta.tsx",
    "src/slot.meta.tsx",
    "src/auto_layout.meta.tsx",
];

fn contents() -> Vec<(&'static str, Content)> {
    FILES.iter().map(|file| (*file, read(file))).collect()
}

/// A section's title with its state names, in order.
fn outline(content: &Content) -> Vec<(&str, Vec<&str>)> {
    content
        .sections
        .iter()
        .map(|section| {
            (
                section.title.as_str(),
                section.states.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

#[test]
fn the_three_files_describe_themselves_in_sections_only() {
    for (file, content) in contents() {
        assert!(
            !content.has_states,
            "{file} keeps a top-level `states` beside its sections"
        );
        assert!(!content.description.trim().is_empty(), "{file}");
        assert!(!content.sections.is_empty(), "{file}");
        for section in &content.sections {
            assert!(!section.title.trim().is_empty(), "{file}");
            assert!(
                !section.description.trim().is_empty(),
                "{file}: the section `{}` has an empty description",
                section.title
            );
            assert!(!section.states.is_empty(), "{file}: `{}`", section.title);
        }
    }
}

#[test]
fn buttons_sections_are_the_ones_asked_for_in_order() {
    let button = read("src/button.meta.tsx");
    assert_eq!(
        outline(&button),
        vec![
            (
                "Solid Variants",
                vec!["Neutral", "Primary", "Danger", "Success"]
            ),
            (
                "Outlined Variants",
                vec![
                    "Outlined",
                    "Primary outlined",
                    "Danger outlined",
                    "Success outlined"
                ]
            ),
            ("Other Variants", vec!["Ghost", "Link"]),
            (
                "Disabled States",
                vec![
                    "Disabled",
                    "Primary disabled",
                    "Danger disabled",
                    "Success disabled"
                ]
            ),
            ("Sizes", vec!["Small", "Large"]),
        ]
    );
}

#[test]
fn slots_and_auto_layouts_sections_are_the_ones_asked_for_in_order() {
    assert_eq!(
        outline(&read("src/slot.meta.tsx")),
        vec![("Named slots", vec!["Empty", "Footer"])]
    );
    assert_eq!(
        outline(&read("src/auto_layout.meta.tsx")),
        vec![
            ("Direction", vec!["Vertical", "Horizontal", "Wrap"]),
            ("Alignment", vec!["Centered", "Space between"]),
            ("Sizing", vec!["Fill"]),
        ]
    );
}

#[test]
fn every_name_of_the_baseline_is_kept_in_its_entry_of_its_file() {
    let text =
        std::fs::read_to_string(crate_dir().join("tests/fixtures/meta-states.json")).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&text).unwrap();
    let files = baseline["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        FILES.len(),
        "the baseline lists the three files"
    );
    for file in files {
        let path = file["file"].as_str().unwrap();
        let content = read(path);
        for entry in file["entries"].as_array().unwrap() {
            let id = entry["id"].as_str().unwrap();
            assert_eq!(content.id, id, "{path}: the entry `{id}`");
            let now: Vec<&str> = content
                .sections
                .iter()
                .flat_map(|section| section.states.iter().map(String::as_str))
                .collect();
            for name in entry["states"].as_array().unwrap() {
                let name = name.as_str().unwrap();
                assert!(
                    now.contains(&name),
                    "{path}: the state `{name}` of `{id}` is gone (now: {now:?})"
                );
            }
        }
    }
}

/// What a generator writes where a description goes, and what a draft leaves.
fn placeholder(text: &str) -> bool {
    let text = text.trim();
    text.contains("purpose here") || text.contains("...") || text.contains('…') || text == "TODO"
}

#[test]
fn no_description_is_empty_a_placeholder_or_its_titles_echo() {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    for (file, content) in contents() {
        assert!(
            !placeholder(&content.description),
            "{file}: {}",
            content.description
        );
        if let Some(other) = seen.insert(content.description.clone(), file) {
            panic!("{file} and {other} share a description");
        }
        for section in &content.sections {
            let text = section.description.trim();
            assert!(!text.is_empty(), "{file}: `{}`", section.title);
            assert!(!placeholder(text), "{file}: {text}");
            assert!(
                !text.eq_ignore_ascii_case(section.title.trim()),
                "{file}: the description of `{}` is its title",
                section.title
            );
            assert!(
                !["states", "default"].contains(&section.title.trim().to_lowercase().as_str()),
                "{file}: a section called `{}`",
                section.title
            );
            if let Some(other) = seen.insert(section.description.clone(), file) {
                panic!(
                    "{file}: `{}` shares its description with {other}: {text}",
                    section.title
                );
            }
        }
    }
}
