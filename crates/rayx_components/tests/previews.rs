//! The previews RayX's components ship: every component the library's set registers has a
//! `<snake_name>.preview.tsx` beside its source, a literal `Previews` object a design tool reads
//! without running it (a literal, non-empty `id` unique in the crate, a literal `title` and
//! `group`, the component imported by name from `rayx/components`, named states whose props are
//! literals); every preview file in the crate is one of a registered component; each compiles
//! against the library's set alone, and in an app serving the set every state of it renders.

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

/// What a preview file declares, as a tool reads it.
#[derive(Debug)]
struct Preview {
    file: PathBuf,
    id: String,
    /// The component's local name and where it is imported from, with its name there.
    component: (String, String, String),
    /// The states' names, in order.
    states: Vec<String>,
}

/// The crate's `src/` folder.
fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `*.preview.tsx` file under `dir`, sorted.
fn preview_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(preview_files(&path));
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".preview.tsx"))
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

/// The preview file each registered component must have, by its export: `<snake_name>.preview.tsx`
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
            folder.join(format!("{}.preview.tsx", snake(&spec.export))),
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
    let Some(Expression::Identifier(component)) =
        field("component").map(Expression::get_inner_expression)
    else {
        return Err("its `component` is not an imported component's name".into());
    };
    let component = imports
        .into_iter()
        .find(|(local, _, _)| local.as_str() == component.name.as_str())
        .ok_or("its `component` is not imported by name")?;
    let Some(Expression::ObjectExpression(states)) =
        field("states").map(Expression::get_inner_expression)
    else {
        return Err("its `states` is not an object literal".into());
    };
    let states = properties(states).ok_or("its states are not written out")?;
    if states.is_empty() {
        return Err("it has no state".into());
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
    Ok(Preview {
        file: file.to_path_buf(),
        id,
        component,
        states: names,
    })
}

/// Every preview file of the crate, read, by file.
fn previews() -> Vec<Preview> {
    preview_files(&src())
        .iter()
        .map(|file| read(file).unwrap_or_else(|error| panic!("{}: {error}", file.display())))
        .collect()
}

#[test]
fn every_component_ships_a_literal_preview_file_beside_its_source() {
    let expected = expected_files();
    let previews = previews();
    for (export, file) in &expected {
        let preview = previews
            .iter()
            .find(|preview| &preview.file == file)
            .unwrap_or_else(|| panic!("{export} has no preview file at {}", file.display()));
        assert_eq!(
            (preview.component.1.as_str(), preview.component.2.as_str()),
            (rayx_components::SPECIFIER, export.as_str()),
            "{}: its component is `{export}`, imported from `rayx/components`",
            file.display()
        );
    }
    // No preview file of a component the set does not register (a stale or misnamed one).
    for preview in &previews {
        assert!(
            expected.values().any(|file| file == &preview.file),
            "{} is no registered component's preview file (expected: {:?})",
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

#[test]
fn a_file_with_code_for_a_literal_is_refused() {
    let dir = std::env::temp_dir().join(format!("rayx-previews-refused-{}", std::process::id()));
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
    ];
    for (name, body) in refused {
        let file = dir.join(format!("{name}.preview.tsx"));
        std::fs::write(
            &file,
            format!("import {{ Button }} from \"rayx/components\";\n{body}\n"),
        )
        .unwrap();
        assert!(read(&file).is_err(), "{name} is refused");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_preview_file_compiles_against_the_library_set_alone() {
    let library = [SetDescription::from(&rayx_components::set())];
    for file in preview_files(&src()) {
        let text = std::fs::read_to_string(&file).unwrap();
        let name = file.file_name().unwrap().to_str().unwrap();
        nsh_bundler::compile_module(name, &text, &library)
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    }
}

/// An app whose entry shows every state of the preview file `preview.tsx` beside it, each under
/// a text naming it.
const SHOW_STATES: &str = r#"import { mount, Text, View } from "nsh";
import previews from "./preview.tsx";

const Shown = previews.component;
const states = Object.entries(previews.states);
mount(() => (
  <View>
    {states.map(([name, state]) => (
      <View>
        <Text>{`state ${name}`}</Text>
        <Shown {...(state.props ?? {})} />
      </View>
    ))}
  </View>
));
"#;

#[test]
fn in_an_app_serving_the_set_every_state_renders() {
    for preview in previews() {
        let dir = std::env::temp_dir().join(format!(
            "rayx-previews-{}-{}",
            std::process::id(),
            preview.id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&preview.file, dir.join("preview.tsx")).unwrap();
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
        let labels: Vec<String> = preview
            .states
            .iter()
            .map(|name| format!("state {name}"))
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
}
