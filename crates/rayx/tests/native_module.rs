//! `#[rayx::native_module]` and `#[rayx::app_extension]`: an app crate's UI set holds RayX's TS
//! classes, the set of every service crate the umbrella's features enable and what its function's
//! body adds, nothing else, and serves RayX's component library, the set `rayx/components` (the
//! components, the previews' types, RayX's theme document), which its TSX imports `Button` from;
//! an extension is a TS function that `builder.apply(..)` runs in chain order.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/enabled.rs"]
mod enabled;
#[path = "../../rayx_app_nsh/tests/headless/mod.rs"]
mod headless;

use std::collections::BTreeSet;

use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx::app_nsh::ts::ServiceProvider;
use rayx::prelude::*;

/// The forecast an extension registers.
#[derive(AppService)]
struct Forecast(&'static str);

/// Registers a sunny forecast.
#[rayx::app_extension]
fn add_sunny(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Forecast("sunny"))
}

/// Registers a rainy forecast.
#[rayx::app_extension]
pub fn add_rainy(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Forecast("rainy"))
}

/// The time, as TS tells it.
#[rayx::contract]
pub trait Clock {
    /// The time now.
    async fn now(&self) -> Result<String, String>;
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The forecast the app registered.
    pub(crate) fn forecast(services: Instance<ServiceProvider>) -> Result<String, String> {
        services
            .with(|services| services.services().required_service::<Forecast>())
            .map(|forecast| forecast.0.to_owned())
            .map_err(|error| error.to_string())
    }
}

#[rayx::native_module("test/app")]
fn native(registry: &mut Registry) {
    registry
        .module::<Probe>()
        .class_extension::<add_sunny>()
        .class_extension::<add_rainy>()
        .contract::<dyn Clock>()
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
fn the_set_holds_rayx_and_what_the_body_adds_and_nothing_else() {
    let set = registry();
    let mut rayx = Registry::ui_set("test/app");
    rayx::app_nsh::register(&mut rayx);
    // The harness's `builder.addHostFakes()`, which the body adds too.
    rayx.class_extension::<headless::HostFakes>();
    enabled::register_service_sets(&mut rayx);
    let mut expected = exports(&rayx);
    expected.extend(
        [
            "module Probe",
            "method RayXAppBuilder.addSunny",
            "method RayXAppBuilder.addRainy",
            "interface Clock",
            "module ClockToken",
        ]
        .map(str::to_owned),
    );
    assert_eq!(exports(&set), expected);
    let exported = exports(&set);
    for name in [
        "class RayXAppBuilder",
        "class ServiceProvider",
        "class ServiceToken",
        "class OptionsToken",
        "class OptionsBuilder",
        "interface AsyncInitializable",
        "interface StartupInitializable",
        "class StartupInitializationState",
        "class StartupInitializationContext",
        "class ReadyContext",
        "type ServiceKey<T>",
    ] {
        assert!(exported.contains(name), "no {name} in {exported:?}");
    }
    // RayX's components are the library's, not the app's: the set serves `rayx/components`.
    for name in [
        "component rayx_components::button::Button",
        "type Previews<C>",
        "type PrivatePreviews<C>",
        "type PreviewState<P>",
    ] {
        assert!(!exported.contains(name), "{name} in the app's set");
    }
    assert!(
        set.theme_documents().is_empty(),
        "no theme document of the app's"
    );
    let sets: Vec<Option<&str>> = set.sets().iter().map(|s| s.specifier()).collect();
    assert_eq!(sets, [Some("test/app"), Some("rayx/components")]);
    let library = set.sets()[1];
    let components = rayx::components::set();
    assert_eq!(exports(library), exports(&components));
    for name in [
        "component rayx_components::button::Button",
        "type Previews<C>",
        "type PrivatePreviews<C>",
        "type PreviewState<P>",
    ] {
        assert!(
            exports(library).contains(name),
            "no {name} in rayx/components"
        );
    }
    assert_eq!(
        library.theme_documents(),
        components.theme_documents(),
        "RayX's theme document"
    );
    assert!(!library.theme_documents().is_empty());
    assert_eq!(set.specifier(), Some("test/app"));
}

const BUTTON_TSX: &str = r#"import { mount, View } from "nsh";
import { Button } from "rayx/components";

mount(() => (
  <View>
    <Button text="done: a Button from rayx/components" variant="primary" />
  </View>
));
"#;

#[test]
fn the_app_s_tsx_imports_rayx_s_components_from_rayx_components() {
    assert_eq!(
        headless::run(registry(), BUTTON_TSX),
        "done: a Button from rayx/components"
    );
    let ts = nsh_core::element::declarations(&registry(), &[]);
    assert!(!ts.contains("export declare const Button"), "{ts}");
}

#[test]
fn the_declarations_declare_the_extensions_and_apply() {
    let ts = nsh_core::element::declarations(&registry(), &[]);
    for declared in [
        "  /** Registers a sunny forecast. */\n  addSunny(): RayXAppBuilder;",
        "  addRainy(): RayXAppBuilder;",
        "  apply(extension: (builder: RayXAppBuilder) => RayXAppBuilder): this;",
        "export declare function clockToken(): ServiceToken<Clock>;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(registry().undocumented(), Vec::<String>::new());
}

#[test]
fn an_extension_is_plain_rust_too() {
    let services = RayXAppBuilder::new()
        .apply(add_sunny)
        .apply(add_rainy)
        .build()
        .unwrap();
    assert_eq!(services.required_service::<Forecast>().unwrap().0, "rainy");
    let services = add_sunny(RayXAppBuilder::new()).build().unwrap();
    assert_eq!(services.required_service::<Forecast>().unwrap().0, "sunny");
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  try {
    // Extensions run in chain order, `apply`'d ones too: the later registration wins.
    const builder = new RayXAppBuilder().addHostFakes().addRainy().defaultInitializationTimeout(1000).addSunny();
    lines.push(String(builder.apply((b) => b.addRainy()) === builder));
    lines.push(thrown(() => builder.apply(() => { throw new Error("no weather"); })));
    const services = await builder.start();
    lines.push(Probe.forecast(services));
    lines.push(thrown(() => builder.addRainy()).includes("started already") ? "started" : "open");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_extension_applied_from_ts_registers_its_service_in_chain_order() {
    assert_eq!(
        headless::run(registry(), TSX),
        "done true|no weather|rainy|started"
    );
}
