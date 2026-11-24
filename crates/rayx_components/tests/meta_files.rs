//! The meta files RayX's components ship: every component the library's set registers has a
//! `<snake_name>.meta.tsx` beside its source, a literal `ComponentMeta` object a design tool reads
//! without running it (a literal, non-empty `id` unique in the crate, a literal `title` and
//! `group`, the component imported by name from `rayx/components`, named states whose props are
//! literals, and, where it has them, a literal `size` and `private` previews as literal as
//! itself, their ids unique among them); every meta file in the crate is one of a registered
//! component; each compiles against the library's set alone, and in an app serving the set
//! every state of it renders.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nsh_core::element::description::SetDescription;
use nsh_engine::{App, Bundle};
use oxc::allocator::Allocator;
use oxc::ast::ast::{
    ArrayExpressionElement, Expression, ImportDeclarationSpecifier, ObjectExpression,
    ObjectPropertyKind, PropertyKey, Statement, UnaryOperator,
};
use oxc::parser::Parser;
use oxc::span::SourceType;
use support::Running;

/// What a meta file declares, as a tool reads it.
#[derive(Debug)]
struct Preview {
    file: PathBuf,
    id: String,
    /// The component's local name and where it is imported from, with its name there.
    component: (String, String, String),
    /// The top level's states' names, in order, across its sections.
    states: Vec<String>,
    /// Each private entry's id and its states' names, in order, across its sections.
    private: Vec<(String, Vec<String>)>,
}

/// The crate's `src/` folder.
fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `*.meta.tsx` file under `dir`, sorted.
fn meta_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(meta_files(&path));
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".meta.tsx"))
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// `IconButton` as `icon_button`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// The meta file each registered component must have, by its export: `<snake_name>.meta.tsx`
/// in the folder of its source (`rayx_components::button::Button` is `src/button.rs`).
fn expected_files() -> BTreeMap<String, PathBuf> {
    let set = rayx_components::set();
    let mut expected = BTreeMap::new();
    for spec in set.specs() {
        let path: Vec<&str> = spec.name.split("::").collect();
        assert_eq!(path.first(), Some(&"rayx_components"), "{}", spec.name);
        let modules = &path[1..path.len() - 1];
        assert!(
            !modules.is_empty(),
            "{} is in a module of its own",
            spec.name
        );
        let file = src().join(modules.join("/"));
        let (source, folder) = if file.with_extension("rs").is_file() {
            (
                file.with_extension("rs"),
                file.parent().unwrap().to_path_buf(),
            )
        } else {
            (file.join("mod.rs"), file.clone())
        };
        assert!(
            source.is_file(),
            "{}: no source at {}",
            spec.name,
            source.display()
        );
        expected.insert(
            spec.export.clone(),
            folder.join(format!("{}.meta.tsx", snake(&spec.export))),
        );
    }
    assert!(!expected.is_empty(), "the set registers components");
    expected
}

fn key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

fn string(value: &Expression) -> Option<String> {
    match value.get_inner_expression() {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) => t.single_quasi().map(|q| q.to_string()),
        _ => None,
    }
}

/// Whether `value` is written as a literal: a string, number, boolean or `null`, or an array or
/// object of them.
fn literal(value: &Expression) -> bool {
    match value.get_inner_expression() {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(t) => t.single_quasi().is_some(),
        Expression::UnaryExpression(u) => {
            u.operator == UnaryOperator::UnaryNegation
                && matches!(
                    u.argument.get_inner_expression(),
                    Expression::NumericLiteral(_)
                )
        }
        Expression::ArrayExpression(a) => a.elements.iter().all(|e| match e {
            ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => false,
            other => other.as_expression().is_some_and(literal),
        }),
        Expression::ObjectExpression(o) => properties(o)
            .is_some_and(|properties| properties.iter().all(|(_, value)| literal(value))),
        _ => false,
    }
}

/// An object's properties by name, when each is a plain `name: value` (no spread, no computed
/// name, no method, no shorthand).
fn properties<'a, 'b>(
    object: &'b ObjectExpression<'a>,
) -> Option<Vec<(String, &'b Expression<'a>)>> {
    object
        .properties
        .iter()
        .map(|property| match property {
            ObjectPropertyKind::ObjectProperty(p) if !p.computed && !p.method && !p.shorthand => {
                Some((key_name(&p.key)?, &p.value))
            }
            _ => None,
        })
        .collect()
}

/// The names of the states `states` declares, in order: an object literal of at least one state,
/// each an object literal whose props are literals. `of` names whose they are in an error.
fn state_names(states: Option<&Expression>, of: &str) -> Result<Vec<String>, String> {
    let Some(Expression::ObjectExpression(states)) = states.map(Expression::get_inner_expression)
    else {
        return Err(format!("{of} `states` is not an object literal"));
    };
    let states = properties(states).ok_or(format!("{of} states are not written out"))?;
    if states.is_empty() {
        return Err(format!("{of} `states` has no state"));
    }
    let mut names = Vec::new();
    for (name, state) in states {
        let Expression::ObjectExpression(state) = state.get_inner_expression() else {
            return Err(format!("state `{name}` is not an object literal"));
        };
        let state = properties(state).ok_or(format!("state `{name}` is not written out"))?;
        for (key, value) in state {
            if key == "props" && !literal(value) {
                return Err(format!("the props of state `{name}` are not all literals"));
            }
        }
        names.push(name);
    }
    Ok(names)
}

/// The names of an entry's states (the file's top level, or one private entry, `entry` its id),
/// in order: either its `states` or its `sections`, never both, each section an object literal
/// of a literal `title`, an optional literal `description` and its states, a state's name
/// written once in the entry across its sections. `of` names whose they are in an error.
fn entry_states(
    states: Option<&Expression>,
    sections: Option<&Expression>,
    entry: &str,
    of: &str,
) -> Result<Vec<String>, String> {
    let names = match (states, sections) {
        (Some(_), Some(_)) => {
            return Err(format!("entry `{entry}` has both `states` and `sections`"));
        }
        (states, None) => state_names(states, of)?,
        (None, Some(sections)) => {
            let Expression::ArrayExpression(list) = sections.get_inner_expression() else {
                return Err(format!("{of} `sections` is not an array literal"));
            };
            if list.elements.is_empty() {
                return Err(format!("{of} `sections` has no section"));
            }
            let mut names = Vec::new();
            for (index, section) in list.elements.iter().enumerate() {
                let of = format!("{of} section {index}'s");
                let Some(Expression::ObjectExpression(section)) = section
                    .as_expression()
                    .map(Expression::get_inner_expression)
                else {
                    return Err(format!("{of} object is not an object literal"));
                };
                let fields =
                    properties(section).ok_or(format!("{of} object is not written out"))?;
                if let Some((name, _)) = fields
                    .iter()
                    .find(|(name, _)| !["title", "description", "states"].contains(&name.as_str()))
                {
                    return Err(format!("{of} `{name}` is no member of a section"));
                }
                let field = |name: &str| {
                    fields
                        .iter()
                        .find(|(key, _)| key == name)
                        .map(|(_, value)| *value)
                };
                field("title")
                    .and_then(string)
                    .ok_or(format!("{of} `title` is missing or not a string literal"))?;
                if field("description").is_some_and(|value| string(value).is_none()) {
                    return Err(format!("{of} `description` is not a string literal"));
                }
                names.extend(state_names(field("states"), &of)?);
            }
            names
        }
    };
    let mut seen: Vec<&String> = Vec::new();
    for name in &names {
        if seen.contains(&name) {
            return Err(format!("entry `{entry}` names the state `{name}` twice"));
        }
        seen.push(name);
    }
    Ok(names)
}

/// Checks a `size`: an object literal of a `width` and a `height`, each a number literal that
/// is not negative, and nothing else.
fn size(value: &Expression) -> Result<(), String> {
    let Expression::ObjectExpression(size) = value.get_inner_expression() else {
        return Err("its `size` is not an object literal".into());
    };
    let size = properties(size).ok_or("its `size` is not written out")?;
    let mut names: Vec<&str> = size.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    if names != ["height", "width"] {
        return Err("its `size` is not a `width` and a `height`".into());
    }
    for (name, value) in size {
        if !matches!(value.get_inner_expression(), Expression::NumericLiteral(_)) {
            return Err(format!("its size's `{name}` is not a number literal"));
        }
    }
    Ok(())
}

/// Checks a `private` list: an array literal of previews as literal as the file's own, each an
/// object literal (written `{ … } satisfies PrivateMeta<typeof Part>`) with a literal,
/// non-empty `id` unique in the list, a literal `title` if it has one, a component imported by
/// name, its states, and nothing else.
fn private_previews(
    value: &Expression,
    imports: &[(String, String, String)],
) -> Result<Vec<(String, Vec<String>)>, String> {
    let Expression::ArrayExpression(list) = value.get_inner_expression() else {
        return Err("its `private` is not an array literal".into());
    };
    let mut ids: Vec<String> = Vec::new();
    let mut entries = Vec::new();
    for (index, entry) in list.elements.iter().enumerate() {
        let of = format!("its private preview {index}'s");
        let Some(Expression::ObjectExpression(entry)) =
            entry.as_expression().map(Expression::get_inner_expression)
        else {
            return Err(format!(
                "its private preview {index} is not an object literal"
            ));
        };
        let fields =
            properties(entry).ok_or(format!("its private preview {index} is not written out"))?;
        if let Some((name, _)) = fields.iter().find(|(name, _)| {
            ![
                "id",
                "title",
                "description",
                "tags",
                "component",
                "states",
                "sections",
            ]
            .contains(&name.as_str())
        }) {
            return Err(format!("{of} `{name}` is no member of a private preview"));
        }
        let field = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| *value)
        };
        let id = field("id")
            .and_then(string)
            .filter(|id| !id.trim().is_empty())
            .ok_or(format!("{of} `id` is not a string literal, or is empty"))?;
        if ids.contains(&id) {
            return Err(format!("two of its private previews have the id `{id}`"));
        }
        ids.push(id.clone());
        for name in ["title", "description"] {
            if field(name).is_some_and(|value| string(value).is_none()) {
                return Err(format!("{of} `{name}` is not a string literal"));
            }
        }
        if field("tags").is_some_and(|tags| {
            !matches!(tags.get_inner_expression(), Expression::ArrayExpression(_)) || !literal(tags)
        }) {
            return Err(format!("{of} `tags` is not an array literal of strings"));
        }
        let Some(Expression::Identifier(component)) =
            field("component").map(Expression::get_inner_expression)
        else {
            return Err(format!(
                "{of} `component` is not an imported component's name"
            ));
        };
        if !imports
            .iter()
            .any(|(local, _, _)| local.as_str() == component.name.as_str())
        {
            return Err(format!("{of} `component` is not imported by name"));
        }
        let states = entry_states(field("states"), field("sections"), &id, &of)?;
        entries.push((id, states));
    }
    Ok(entries)
}

/// Reads `file` as a tool does, failing with what is not literal.
fn read(file: &Path) -> Result<Preview, String> {
    let text = std::fs::read_to_string(file).unwrap();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &text, SourceType::tsx()).parse();
    if let Some(error) = parsed.diagnostics.errors().next() {
        return Err(error.to_string());
    }
    let mut imports: Vec<(String, String, String)> = Vec::new();
    let mut exported = None;
    for statement in &parsed.program.body {
        match statement {
            Statement::ImportDeclaration(import) => {
                for specifier in import.specifiers.iter().flatten() {
                    if let ImportDeclarationSpecifier::ImportSpecifier(s) = specifier {
                        imports.push((
                            s.local.name.to_string(),
                            import.source.value.to_string(),
                            s.imported.name().to_string(),
                        ));
                    }
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                let object = export
                    .declaration
                    .as_expression()
                    .map(Expression::get_inner_expression);
                let Some(Expression::ObjectExpression(object)) = object else {
                    return Err("its default export is not an object literal".into());
                };
                exported = Some(object);
            }
            _ => {}
        }
    }
    let object = exported.ok_or("it has no default export")?;
    let fields =
        properties(object).ok_or("its object is not written out: no spread, no computed name")?;
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| *value)
    };
    let id = field("id")
        .and_then(string)
        .ok_or("its `id` is not a string literal")?;
    if id.trim().is_empty() {
        return Err("its `id` is empty".into());
    }
    for name in ["title", "group"] {
        let value = field(name).ok_or(format!("it has no `{name}`"))?;
        string(value).ok_or(format!("its `{name}` is not a string literal"))?;
    }
    if field("description").is_some_and(|value| string(value).is_none()) {
        return Err("its `description` is not a string literal".into());
    }
    if let Some(value) = field("size") {
        size(value)?;
    }
    let Some(Expression::Identifier(component)) =
        field("component").map(Expression::get_inner_expression)
    else {
        return Err("its `component` is not an imported component's name".into());
    };
    let private = match field("private") {
        Some(value) => private_previews(value, &imports)?,
        None => Vec::new(),
    };
    let component = imports
        .into_iter()
        .find(|(local, _, _)| local.as_str() == component.name.as_str())
        .ok_or("its `component` is not imported by name")?;
    let states = entry_states(field("states"), field("sections"), &id, "its")?;
    Ok(Preview {
        file: file.to_path_buf(),
        id,
        component,
        states,
        private,
    })
}

/// Every meta file of the crate, read, by file.
fn previews() -> Vec<Preview> {
    meta_files(&src())
        .iter()
        .map(|file| read(file).unwrap_or_else(|error| panic!("{}: {error}", file.display())))
        .collect()
}

#[test]
fn every_component_ships_a_literal_meta_file_beside_its_source() {
    let expected = expected_files();
    let previews = previews();
    for (export, file) in &expected {
        let preview = previews
            .iter()
            .find(|preview| &preview.file == file)
            .unwrap_or_else(|| panic!("{export} has no meta file at {}", file.display()));
        assert_eq!(
            (preview.component.1.as_str(), preview.component.2.as_str()),
            (rayx_components::SPECIFIER, export.as_str()),
            "{}: its component is `{export}`, imported from `rayx/components`",
            file.display()
        );
    }
    // No meta file of a component the set does not register (a stale or misnamed one).
    for preview in &previews {
        assert!(
            expected.values().any(|file| file == &preview.file),
            "{} is no registered component's meta file (expected: {:?})",
            preview.file.display(),
            expected.values().collect::<Vec<_>>()
        );
    }
    // Ids are unique in the crate.
    let mut ids: BTreeMap<&str, &Path> = BTreeMap::new();
    for preview in &previews {
        if let Some(other) = ids.insert(&preview.id, &preview.file) {
            panic!(
                "id `{}` is both {} and {}",
                preview.id,
                other.display(),
                preview.file.display()
            );
        }
    }
}

/// The sub-extension meta files had before, built from parts so that no source line holds it.
fn former_suffix() -> String {
    [".prev", "iew.tsx"].concat()
}

#[test]
fn a_file_of_the_former_sub_extension_is_no_meta_file() {
    // None is left in the crate.
    let mut left = Vec::new();
    let mut folders = vec![src()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&former_suffix()))
            {
                left.push(path);
            }
        }
    }
    assert_eq!(
        left,
        Vec::<PathBuf>::new(),
        "files of the former sub-extension"
    );
    // Beside a component's source, one does not count as its meta file: only `<snake_name>.meta.tsx`.
    let dir = std::env::temp_dir().join(format!("rayx-meta-former-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let former = dir.join(format!("button{}", former_suffix()));
    std::fs::write(&former, "export default {};\n").unwrap();
    assert_eq!(
        meta_files(&dir),
        Vec::<PathBuf>::new(),
        "{}",
        former.display()
    );
    let current = dir.join("button.meta.tsx");
    std::fs::write(&current, "export default {};\n").unwrap();
    assert_eq!(meta_files(&dir), vec![current]);
    let _ = std::fs::remove_dir_all(&dir);
    for file in expected_files().values() {
        assert!(
            file.to_str()
                .is_some_and(|name| name.ends_with(".meta.tsx")),
            "{}",
            file.display()
        );
    }
}

#[test]
fn a_file_with_code_for_a_literal_is_refused() {
    let dir = std::env::temp_dir().join(format!("rayx-meta-refused-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let refused = [
        (
            "no-id",
            "export default { title: \"A\", group: \"B\", component: Button, states: {} };",
        ),
        (
            "code-id",
            "const id = \"button\";\nexport default { id, title: \"A\", group: \"B\", component: Button, states: { A: {} } };",
        ),
        (
            "code-prop",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: { props: { text: String(1) } } } };",
        ),
        (
            "no-state",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: {} };",
        ),
        (
            "code-size",
            "const width = 390;\nexport default { id: \"button\", title: \"A\", group: \"B\", size: { width, height: 844 }, component: Button, states: { A: {} } };",
        ),
        (
            "computed-size",
            "export default { id: \"button\", title: \"A\", group: \"B\", size: { width: 390, height: 2 * 422 }, component: Button, states: { A: {} } };",
        ),
        (
            "half-a-size",
            "export default { id: \"button\", title: \"A\", group: \"B\", size: { width: 390 }, component: Button, states: { A: {} } };",
        ),
        (
            "private-code",
            "const parts = [];\nexport default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: parts };",
        ),
        (
            "private-spread",
            "const parts = [];\nexport default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [...parts] };",
        ),
        (
            "private-code-prop",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [{ id: \"part\", component: Button, states: { A: { props: { text: String(1) } } } }] };",
        ),
        (
            "private-no-id",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [{ component: Button, states: { A: {} } }] };",
        ),
        (
            "private-same-id",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [{ id: \"part\", component: Button, states: { A: {} } }, { id: \"part\", component: Button, states: { B: {} } }] };",
        ),
        (
            "private-no-state",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [{ id: \"part\", component: Button, states: {} }] };",
        ),
        (
            "private-not-imported",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [{ id: \"part\", component: Part, states: { A: {} } }] };",
        ),
        (
            "private-nested",
            "export default { id: \"button\", title: \"A\", group: \"B\", component: Button, states: { A: {} }, private: [{ id: \"part\", component: Button, states: { A: {} }, private: [] }] };",
        ),
    ];
    for (name, body) in refused {
        let file = dir.join(format!("{name}.meta.tsx"));
        std::fs::write(
            &file,
            format!("import {{ Button }} from \"rayx/components\";\n{body}\n"),
        )
        .unwrap();
        assert!(read(&file).is_err(), "{name} is refused");
    }
    // The same file with a literal size and literal private previews is read.
    let file = dir.join("read.meta.tsx");
    std::fs::write(
        &file,
        "import { Button, type ComponentMeta, type PrivateMeta } from \"rayx/components\";\n\
         export default { id: \"button\", title: \"A\", group: \"B\", size: { width: 390, height: 844.5 }, \
         component: Button, states: { A: {} }, private: [\
         { id: \"part\", title: \"Part\", component: Button, states: { A: { props: { text: \"A\" } } } } \
         satisfies PrivateMeta<typeof Button>, \
         { id: \"other\", component: Button, states: { A: {} } } satisfies PrivateMeta<typeof Button>] } \
         satisfies ComponentMeta<typeof Button>;\n",
    )
    .unwrap();
    let preview = read(&file).unwrap_or_else(|error| panic!("read.meta.tsx: {error}"));
    assert_eq!((preview.id.as_str(), preview.states.len()), ("button", 1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_meta_file_compiles_against_the_library_set_alone() {
    let library = [SetDescription::from(&rayx_components::set())];
    for file in meta_files(&src()) {
        let text = std::fs::read_to_string(&file).unwrap();
        let name = file.file_name().unwrap().to_str().unwrap();
        nsh_bundler::compile_module(name, &text, &library)
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    }
}

/// An app whose entry shows every state of the meta file `meta.tsx` beside it, each under
/// a text naming it.
const SHOW_STATES: &str = r#"import { mount, Text, View } from "nsh";
import meta from "./meta.tsx";

// An entry's states, from its `states` or across its `sections`, in order.
const statesOf = (entry) =>
  entry.states
    ? Object.entries(entry.states)
    : entry.sections.flatMap((section) => Object.entries(section.states));
// Every state of the top level and of each private entry, with its component and its label.
const shown = [
  ...statesOf(meta).map(([name, state]) => [meta.component, `state ${name}`, state]),
  ...(meta.private ?? []).flatMap((entry) =>
    statesOf(entry).map(([name, state]) => [entry.component, `state ${entry.id}/${name}`, state]),
  ),
];
mount(() => (
  <View>
    {shown.map(([Shown, label, state]) => (
      <View>
        <Text>{label}</Text>
        <Shown {...(state.props ?? {})} />
      </View>
    ))}
  </View>
));
"#;

/// A meta file in sections, with a private entry in sections too, which shares a state's name
/// with the top level (two entries may).
const SECTIONED: &str = r#"import { Button, type ComponentMeta, type PrivateMeta } from "rayx/components";

export default {
  id: "button",
  title: "Button",
  group: "Components",
  description: "Starts an action.",
  component: Button,
  sections: [
    {
      title: "Variants",
      description: `One per **kind** of action.`,
      states: {
        Neutral: { props: { text: "Button" } },
        Primary: { props: { text: "Button", variant: "primary" } },
      },
    },
    { title: "Sizes", states: { Small: { props: { text: "Button", size: "small" } } } },
  ],
  private: [
    {
      id: "part",
      title: "Part",
      description: "A part of it.",
      tags: ["part"],
      component: Button,
      sections: [
        { title: "Kinds", states: { Neutral: { props: { text: "Part" } } } },
        { title: "Widths", description: "Wide.", states: { Wide: { props: { text: "Wide part" } } } },
      ],
    } satisfies PrivateMeta<typeof Button>,
  ],
} satisfies ComponentMeta<typeof Button>;
"#;

#[test]
fn sections_are_read_and_their_wrong_shapes_refused() {
    let dir =
        std::env::temp_dir().join(format!("rayx-meta-sections-refused-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let head = "export default { id: \"button\", title: \"A\", group: \"B\", component: Button,";
    // Each refused body, and what its error names (the entry, the state).
    let refused: [(&str, &str, &[&str]); 12] = [
        (
            "both",
            "states: { A: {} }, sections: [{ title: \"S\", states: { B: {} } }] };",
            &["entry `button`", "both `states` and `sections`"],
        ),
        (
            "repeated",
            "sections: [{ title: \"S\", states: { A: {} } }, { title: \"T\", states: { A: {} } }] };",
            &["entry `button`", "`A`"],
        ),
        ("no-sections", "sections: [] };", &["no section"]),
        (
            "sections-code",
            "sections: parts };",
            &["not an array literal"],
        ),
        (
            "no-title",
            "sections: [{ states: { A: {} } }] };",
            &["`title`"],
        ),
        (
            "code-title",
            "sections: [{ title: String(1), states: { A: {} } }] };",
            &["`title`"],
        ),
        (
            "substituted-title",
            "sections: [{ title: `S${1}`, states: { A: {} } }] };",
            &["`title`"],
        ),
        (
            "code-description",
            "description: String(1), states: { A: {} } };",
            &["`description`"],
        ),
        (
            "section-code-description",
            "sections: [{ title: \"S\", description: String(1), states: { A: {} } }] };",
            &["`description`"],
        ),
        (
            "section-code-prop",
            "sections: [{ title: \"S\", states: { A: { props: { text: String(1) } } } }] };",
            &["not all literals"],
        ),
        (
            "section-member",
            "sections: [{ title: \"S\", id: \"s\", states: { A: {} } }] };",
            &["`id` is no member"],
        ),
        (
            "private-repeated",
            "states: { A: {} }, private: [{ id: \"part\", component: Button, sections: [{ title: \"S\", states: { B: {} } }, { title: \"T\", states: { B: {} } }] }] };",
            &["entry `part`", "`B`"],
        ),
    ];
    for (name, body, said) in refused {
        let file = dir.join(format!("{name}.meta.tsx"));
        std::fs::write(
            &file,
            format!(
                "import {{ Button }} from \"rayx/components\";\nconst parts = [];\n{head} {body}\n"
            ),
        )
        .unwrap();
        let error = read(&file)
            .err()
            .unwrap_or_else(|| panic!("{name} is refused"));
        for fragment in said {
            assert!(
                error.contains(fragment),
                "{name}: `{error}` names {fragment}"
            );
        }
    }
    let file = dir.join("private-both.meta.tsx");
    std::fs::write(
        &file,
        format!("import {{ Button }} from \"rayx/components\";\n{head} states: {{ A: {{}} }}, private: [{{ id: \"part\", component: Button, states: {{ A: {{}} }}, sections: [{{ title: \"S\", states: {{ B: {{}} }} }}] }}] }};\n"),
    )
    .unwrap();
    let error = read(&file).expect_err("private-both is refused");
    assert!(error.contains("entry `part` has both"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Shows every state of `preview` (its top level's and its private entries', from `states` or
/// across `sections`) in an app serving the set, each under a text naming it, and checks each
/// mounted its component.
fn renders_every_state(preview: &Preview) {
    // The folder the file is in names the run too: two files may share an id (a test's own).
    let folder = preview.file.parent().and_then(Path::file_name).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "rayx-meta-{}-{}-{}",
        std::process::id(),
        folder.to_string_lossy(),
        preview.id
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&preview.file, dir.join("meta.tsx")).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, SHOW_STATES).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        ui_sets: support::described(),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("{}: {error}", preview.file.display()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = Running::start(App::Js(Bundle::source(bundle.code)));
    let labels: Vec<String> =
        preview
            .states
            .iter()
            .map(|name| format!("state {name}"))
            .chain(preview.private.iter().flat_map(|(id, states)| {
                states.iter().map(move |name| format!("state {id}/{name}"))
            }))
            .collect();
    assert!(
        app.wait(|s| labels.iter().all(|label| s.text_node(label).is_some())),
        "{}: {:?}",
        preview.file.display(),
        app.screen.shown()
    );
    // Each state's component mounted beside its name: the view of its name has a sibling.
    for label in &labels {
        let text = app.screen.text_node(label).unwrap();
        let state = *app.screen.parents.get(&text).unwrap();
        assert!(
            app.wait(|s| s.children(state).len() == 2),
            "{}: `{label}` shows no component: {:?}",
            preview.file.display(),
            app.screen.shown()
        );
    }
}

#[test]
fn in_an_app_serving_the_set_every_state_renders() {
    for preview in previews() {
        renders_every_state(&preview);
    }
}

#[test]
fn every_state_of_every_section_renders() {
    let dir = std::env::temp_dir().join(format!("rayx-meta-sections-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("button.meta.tsx");
    std::fs::write(&file, SECTIONED).unwrap();
    let preview = read(&file).unwrap_or_else(|error| panic!("button.meta.tsx: {error}"));
    assert_eq!(preview.states, ["Neutral", "Primary", "Small"]);
    assert_eq!(
        preview.private,
        [(
            "part".to_owned(),
            vec!["Neutral".to_owned(), "Wide".to_owned()]
        )]
    );
    renders_every_state(&preview);
    let _ = std::fs::remove_dir_all(&dir);
}
