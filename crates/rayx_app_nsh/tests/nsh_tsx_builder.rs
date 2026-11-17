//! TS `RayXAppBuilder` through NSH's engine, headless: every Rust builder method an app calls
//! has a chaining camelCase twin that registers what the Rust method does, with the same implied
//! services, and the set declares each with a doc comment. (The service crates' methods,
//! `addPlatform`, `addAssets`, `addStorage`, `addNavigation`, `addOperations`, `addHistory`,
//! `addI18n`, `addSelection` and the others, are their crates' groups'; the services the host
//! requires come from `addHostFakes()` here, and a key-value store from the test's
//! `addFakeStorage()`.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app_nsh::rayx_runtime::services::storage::KeyValueStore;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_app_nsh::{Configuration, RayXAppBuilder, ServiceProvider as Services};

/// Registers a key-value store in memory, which persistent history needs.
#[rayx_app_nsh::app_extension]
fn add_fake_storage(mut builder: RayXAppBuilder) -> RayXAppBuilder {
    use rayx_app_nsh::AppBuilder as _;
    builder
        .services_mut()
        .add_singleton_as::<dyn KeyValueStore>(
            rayx_app::testing::FakeKeyValueStore::new() as Arc<dyn KeyValueStore>
        );
    builder
}

/// The app's files: its settings.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => {
                Ok(br#"{ "Feed": { "Label": "settings", "Source": "appsettings" } }"#.to_vec())
            }
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// Whether a service resolves.
type Resolves = fn(&Services) -> bool;

/// Whether `T` resolves.
fn resolves<T: ?Sized + Send + Sync + 'static>(services: &Services) -> bool {
    services.required_service::<T>().is_ok()
}

/// What the test reads of the app's services from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The services of RayX's that resolve, by name.
    pub(crate) fn registered(services: Instance<ServiceProvider>) -> Vec<String> {
        services.with(|services| {
            let services = services.services();
            let checks: [(&str, Resolves); 1] = [("KeyValueStore", resolves::<dyn KeyValueStore>)];
            checks
                .into_iter()
                .filter(|(_, resolves)| resolves(services))
                .map(|(name, _)| name.to_owned())
                .collect()
        })
    }

    /// The configuration's value at `path`, `null` when it has none.
    pub(crate) fn setting(services: Instance<ServiceProvider>, path: String) -> JsonValue {
        services.with(|services| {
            services
                .services()
                .required_service::<Configuration>()
                .ok()
                .and_then(|configuration| configuration.value(&path))
                .unwrap_or(JsonValue::Null)
        })
    }

    /// The startup services' default timeout, in milliseconds.
    pub(crate) fn timeout(services: Instance<ServiceProvider>) -> f64 {
        services.with(|services| {
            services
                .services()
                .startup_registry()
                .default_initialization_timeout()
                .as_secs_f64()
                * 1000.0
        })
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>().class_extension::<add_fake_storage>();
    })
}

const EVERY_METHOD: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const builder = new RayXAppBuilder().addHostFakes();
    const chained = builder
      .addAppSettings("app")
      .addOptionalAppSettings("asset:/@app/appsettings.Development.json")
      .configureAppConfiguration((configuration) => {
        configuration
          .addJson('{ "Feed": { "Label": "json" } }')
          .addInMemory({ "Feed:Limit": 3 })
          .addValue({ Feed: { Mode: "live" } });
      })
      .addFakeStorage()
      .defaultInitializationTimeout(1234);
    lines.push(String(chained === builder));
    const services = await builder.start();
    lines.push(Probe.registered(services).join(","));
    lines.push(["Label", "Source", "Limit", "Mode"].map((key) => Probe.setting(services, `Feed:${key}`)).join(","));
    lines.push(String(Probe.timeout(services)));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn each_builder_method_registers_its_service() {
    let shown = headless::run_with_assets(set(), EVERY_METHOD, Arc::new(AppFiles));
    assert_eq!(
        shown,
        "done true\
         |KeyValueStore\
         |json,appsettings,3,live|1234"
    );
}

/// Each app on an engine of its own: what one method adds beyond its own service.
const IMPLIED: &str = r#"import { mount, signal, Text } from "nsh";
import { Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      @CHAIN@
      .start();
    lines.push(Probe.registered(services).join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn builder_methods_add_the_services_they_imply_as_in_rust() {
    // No method registers a key-value store: those kept in one require it.
    for (chain, registered) in [("", "")] {
        let shown = headless::run(set(), &IMPLIED.replace("@CHAIN@", chain));
        assert_eq!(shown, format!("done {registered}"), "{chain}");
    }
}

#[test]
fn the_declarations_declare_each_builder_method_with_a_doc_comment() {
    let set = set();
    let ts = headless::declarations(&set);
    let class = ts
        .split("export declare class RayXAppBuilder {")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .expect("RayXAppBuilder is declared");
    let lines: Vec<&str> = class.lines().collect();
    assert!(class.contains("  constructor();"), "{class}");
    for method in [
        "addAppSettings(package: string): RayXAppBuilder;",
        "addOptionalAppSettings(uri: string): RayXAppBuilder;",
        "configureAppConfiguration(configure: (configuration: ConfigurationBuilder) => void): RayXAppBuilder;",
        "defaultInitializationTimeout(ms: number): RayXAppBuilder;",
        "start(): Promise<ServiceProvider>;",
    ] {
        let at = lines
            .iter()
            .position(|line| line.trim() == method)
            .unwrap_or_else(|| panic!("no `{method}` in\n{class}"));
        assert!(
            lines[at - 1].trim_start().starts_with("/**")
                || lines[at - 1].trim_end().ends_with("*/"),
            "`{method}` has no doc comment"
        );
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
