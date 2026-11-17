//! `#[rayx::native_module("spec")]` on an app crate's registry function: the set holds every RayX
//! TS class and RayX's components with their theme document, then what the function's body adds
//! (a component, a module, an app extension TS applies), and nothing else; its declarations
//! declare them all, documented; and NSH's CLI reads the set's specifier from the attribute's line
//! in the crate's source, as it does for NSH's own `#[native_module]`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../../rayx_app/tests/headless/mod.rs"]
mod headless;

use std::collections::BTreeSet;

use nsh_core::prelude::*;
use rayx::app::ts::ServiceProvider;
use rayx::prelude::*;

/// The greeting an extension registers.
#[derive(AppService)]
struct Greeting(&'static str);

/// Registers a greeting.
#[rayx::app_extension]
fn add_greeting(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Greeting("hello"))
}

/// A badge of the app's own.
#[component]
pub fn Badge() -> impl IntoElement {
    view().child(text("new"))
}

/// The test's Rust side.
#[module]
struct Greeter;

#[module]
impl Greeter {
    /// The greeting the app registered.
    pub(crate) fn greeting(services: Instance<ServiceProvider>) -> Result<String, String> {
        services
            .with(|services| services.services().required_service::<Greeting>())
            .map(|greeting| greeting.0.to_owned())
            .map_err(|error| error.to_string())
    }
}

#[rayx::native_module("test/app")]
fn native(registry: &mut Registry) {
    registry
        .register::<Badge>()
        .module::<Greeter>()
        .module::<add_greeting>();
}

/// What a set exports, by kind and name.
fn exports(set: &Registry) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.extend(
        set.class_specs()
            .iter()
            .map(|c| format!("class {}", c.name)),
    );
    names.extend(
        set.module_specs()
            .iter()
            .map(|m| format!("module {}", m.name)),
    );
    names.extend(set.specs().iter().map(|c| format!("component {}", c.name)));
    names.extend(
        set.interface_defs()
            .iter()
            .map(|i| format!("interface {}", i.name)),
    );
    names.extend(set.ts_aliases().iter().map(|a| format!("type {}", a.name)));
    names
}

#[test]
fn the_set_is_rayx_s_then_the_body_s_exports_and_nothing_else() {
    let set = registry();
    let mut rayx = Registry::ui_set("test/app");
    rayx::app::register(&mut rayx);
    rayx::components::register(&mut rayx);
    let mut expected = exports(&rayx);
    let own: BTreeSet<String> = exports(&set).difference(&expected).cloned().collect();
    assert_eq!(own.len(), 3, "the body's three exports: {own:?}");
    assert!(own.contains("module Greeter"), "{own:?}");
    assert!(own.contains("module add_greeting"), "{own:?}");
    assert!(
        own.iter()
            .any(|name| name.starts_with("component ") && name.ends_with("Badge")),
        "{own:?}"
    );
    expected.extend(own);
    assert_eq!(exports(&set), expected);
    assert!(exports(&set).contains("class RayXAppBuilder"));
    assert_eq!(
        set.theme_documents(),
        rayx.theme_documents(),
        "RayX's theme document"
    );
    assert_eq!(set.specifier(), Some("test/app"));
}

#[test]
fn the_declarations_declare_rayx_s_and_the_body_s_exports_documented() {
    let set = registry();
    let ts = nsh_core::element::declarations(&set, &[]);
    for declared in [
        "export declare class RayXAppBuilder",
        "export declare class ServiceProvider",
        "export declare const Button:",
        "export declare const Badge:",
        "/** Registers a greeting. */\nexport declare function addGreeting(builder: RayXAppBuilder): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { addGreeting, Greeter, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
new RayXAppBuilder()
  .apply(addGreeting)
  .start()
  .then(
    (services) => shown.set(`done ${Greeter.greeting(services)}`),
    (error) => shown.set(`done failed ${error.message}`),
  );
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_extension_the_body_exports_is_applied_from_ts() {
    assert_eq!(headless::run(registry(), TSX), "done hello");
}

/// NSH's CLI's specifier scan of a crate's source, as `specifier_of` in
/// `crates/nsh-cli/src/rust_app.rs` at `nshx-0.5.0-6` (`ddac2279b`) writes it: the first line that,
/// trimmed, starts with `#[` and has `native_module` followed by `("…"`. NSH names the set's
/// web gate after it (`nsh_ui_set_<specifier>`); without one it falls back to the crate's name,
/// and the set's gate is dropped from the web runtime.
fn nsh_specifier_of(code: &str) -> Option<String> {
    code.lines().find_map(|line| {
        const ATTR: &str = "native_module";
        let attr = line.trim().strip_prefix("#[")?;
        let rest = attr[attr.find(ATTR)? + ATTR.len()..].trim_start();
        let rest = rest.strip_prefix('(')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_owned())
    })
}

#[test]
fn nsh_s_cli_reads_the_specifier_from_the_attribute_s_line() {
    let source = include_str!("native_module_attribute.rs");
    assert_eq!(nsh_specifier_of(source).as_deref(), Some("test/app"));
    assert_eq!(
        nsh_specifier_of("use rayx::prelude::*;\n\n#[rayx::native_module(\"my-app/native\")]\nfn native(registry: &mut Registry) {}\n")
            .as_deref(),
        Some("my-app/native")
    );
    // A function-like macro's line is no attribute: NSH would not see the set.
    let macro_form = concat!("rayx::native_module", "!(\"my-app\");\n");
    assert_eq!(nsh_specifier_of(macro_form), None);
}
