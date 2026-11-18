//! `addApplicationConfiguration` in TS through NSH's engine, headless: its options callback sets
//! the settings as Rust's configure closure does (`name`, `description`, `merge`), after those
//! bound from `appsettings.json` (a value it sets overrides the file's, one it leaves alone keeps
//! it), and they reach RayX's service; a callback that throws, or a `merge` of a value of the
//! wrong type, makes `addApplicationConfiguration` throw; the options object is valid during its
//! callback only; the options class is declared with Rust's methods.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app::services::application_configuration::ApplicationConfiguration;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;

/// The app's files: its settings.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => Ok(br#"{ "RayX": {
                "Application": { "Name": "From settings", "Description": "kept" }
            } }"#
                .to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// What the test reads of the app's options from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The settings the service was made with.
    pub(crate) fn options(services: Instance<ServiceProvider>) -> Result<JsonValue, String> {
        let options = services
            .with(|services| services.services().options::<ApplicationConfiguration>())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(options.cloned()).map_err(|error| error.to_string())
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_application_configuration::register(set);
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { ApplicationConfigurationService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addAppSettings("app")
      .addApplicationConfiguration((options) => {
        options.name("From TS").merge({ notifications: { maxItems: 7 } });
      })
      .start();

    const application = Probe.options(services);
    lines.push(`${application.name} ${application.description} ${application.notifications.max_items}`);
    lines.push(services.getRequiredService(ApplicationConfigurationService).current().name);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn options_set_in_ts_apply_after_configuration_and_reach_the_service() {
    let shown = testing::run_with_assets(set(), TSX, Arc::new(AppFiles));
    // Set by the callback over the file's; left alone, the file's.
    assert_eq!(shown, "done From TS kept 7|From TS");
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.addApplicationConfiguration(() => { throw new Error("no name"); })));
  lines.push(thrown(() => builder.addApplicationConfiguration((options) => {
    options.merge({ notifications: { maxItems: "many" } });
  })).startsWith("merge: ") ? "merge refused" : "merge kept");
  // An options object is valid during its callback only.
  let kept;
  builder.addApplicationConfiguration((options) => { kept = options; });
  lines.push(thrown(() => kept.name("Late")).includes("disposed") ? "disposed" : "kept");
  // The builder still works after a callback threw.
  try {
    await builder.addApplicationConfiguration().start();
    lines.push("started");
  } catch (error) {
    lines.push(`rejected ${error.message}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_add_application_configuration_throw() {
    assert_eq!(
        testing::run(set(), THROWING),
        "done no name|merge refused|disposed|started"
    );
}

#[test]
fn the_options_class_is_declared_with_rust_s_methods() {
    let ts = testing::declarations(&set());
    for declared in [
        "export declare class ApplicationConfigurationOptions {",
        "  name(name: string): ApplicationConfigurationOptions;",
        "  merge(settings: Record<string, unknown>): ApplicationConfigurationOptions;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
