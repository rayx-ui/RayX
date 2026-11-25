//! `#[rayx::native_module("spec")]` on an app crate's registry function: the set holds RayX's TS
//! classes and the set of every service crate the umbrella's features enable (and no other's),
//! then what the function's body adds (a component, a module, an app extension TS applies), and
//! nothing else, and serves RayX's components with their theme document as the library's set,
//! `rayx/components`; its declarations declare its own exports,
//! documented; TS's `RayXAppBuilder` has the builder methods of the enabled crates only, in JS and
//! in the declarations (with the default features `addFonts` and no `addI18n`, with every feature
//! both); and NSH's CLI reads the set's specifier from the attribute's line in the crate's source,
//! as it does for NSH's own `#[native_module]`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/enabled.rs"]
mod enabled;
#[path = "../../rayx_app_nsh/tests/headless/mod.rs"]
mod headless;

use std::collections::BTreeSet;

use nsh_core::prelude::*;
use rayx::app_nsh::ts::ServiceProvider;
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
        .class_extension::<add_greeting>()
        // The services the host requires, faked for the test's start.
        .class_extension::<headless::HostFakes>();
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
    names.extend(set.class_extensions().iter().flat_map(|extension| {
        extension
            .members
            .iter()
            .map(|member| format!("method {}.{}", extension.class.name, member.name))
    }));
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
    rayx::app_nsh::register(&mut rayx);
    // The harness's `builder.addHostFakes()`, which the body adds too.
    rayx.class_extension::<headless::HostFakes>();
    enabled::register_service_sets(&mut rayx);
    let mut expected = exports(&rayx);
    let own: BTreeSet<String> = exports(&set).difference(&expected).cloned().collect();
    assert_eq!(own.len(), 3, "the body's three exports: {own:?}");
    assert!(own.contains("module Greeter"), "{own:?}");
    assert!(own.contains("method RayXAppBuilder.addGreeting"), "{own:?}");
    assert!(
        own.iter()
            .any(|name| name.starts_with("component ") && name.ends_with("Badge")),
        "{own:?}"
    );
    expected.extend(own);
    assert_eq!(exports(&set), expected);
    assert!(exports(&set).contains("class RayXAppBuilder"));
    // RayX's components and theme document are the served library's.
    let sets: Vec<Option<&str>> = set.sets().iter().map(|s| s.specifier()).collect();
    assert_eq!(sets, [Some("test/app"), Some(rayx::components::SPECIFIER)]);
    assert!(set.theme_documents().is_empty());
    assert_eq!(
        set.sets()[1].theme_documents(),
        rayx::components::set().theme_documents(),
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
        "export declare const Badge:",
        "  /** Registers a greeting. */\n  addGreeting(): RayXAppBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert!(!ts.contains("export declare const Button:"), "{ts}");
    assert_eq!(set.undocumented(), Vec::<String>::new());
    // The library's set declares RayX's components, documented.
    let library = set.sets()[1];
    let ts = nsh_core::element::declarations(library, &[]);
    assert!(ts.contains("export declare const Button:"), "{ts}");
    assert_eq!(library.undocumented(), Vec::<String>::new());
}

#[test]
fn the_builder_declares_the_methods_of_the_enabled_crates_only() {
    let ts = nsh_core::element::declarations(&registry(), &[]);
    let declares = |method: &str| ts.contains(&format!("\n  {method}("));
    let problems: Vec<String> = enabled::BUILDER_METHODS
        .iter()
        .filter(|(_, method, enabled)| declares(method) != *enabled)
        .map(|(feature, method, enabled)| {
            if *enabled {
                format!("feature {feature} is on and `{method}` is not declared")
            } else {
                format!("feature {feature} is off and `{method}` is declared")
            }
        })
        .collect();
    assert!(problems.is_empty(), "{}\nin\n{ts}", problems.join("\n"));
    // The criterion's two cases: the defaults have `addFonts` and no `addI18n`, every feature
    // both.
    assert_eq!(declares("addFonts"), cfg!(feature = "fonts"));
    assert_eq!(declares("addI18n"), cfg!(feature = "i18n"));
    assert_eq!(
        ts.contains("export declare class I18nService"),
        cfg!(feature = "i18n")
    );
}

/// Shows, for each service's builder method, whether a TS builder has it.
const BUILDER_METHODS_TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(() => {
  const builder = new RayXAppBuilder();
  const methods = METHODS.map((method) => `${method}:${typeof builder[method] === "function"}`);
  shown.set(`done ${methods.join("|")}`);
}, 0);
mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_ts_builder_has_the_methods_of_the_enabled_crates_only() {
    let methods: Vec<String> = enabled::BUILDER_METHODS
        .iter()
        .map(|(_, method, _)| format!("\"{method}\""))
        .collect();
    let tsx = BUILDER_METHODS_TSX.replace("METHODS", &format!("[{}]", methods.join(", ")));
    let expected: Vec<String> = enabled::BUILDER_METHODS
        .iter()
        .map(|(_, method, enabled)| format!("{method}:{enabled}"))
        .collect();
    assert_eq!(
        headless::run(registry(), &tsx),
        format!("done {}", expected.join("|"))
    );
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Greeter, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
new RayXAppBuilder()
  .addHostFakes()
  .addGreeting()
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
/// `crates/nsh-cli/src/rust_app.rs` at `nshx-0.7.0-1` (`d49d11bf3`) writes it: the first line that,
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
