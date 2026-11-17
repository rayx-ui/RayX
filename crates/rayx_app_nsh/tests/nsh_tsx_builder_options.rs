//! Options callbacks of TS `RayXAppBuilder` methods through NSH's engine, headless: the options
//! object has Rust's methods in camelCase, what it sets applies after the options bound from
//! `appsettings.json` (a value it sets overrides the file's, one it leaves alone keeps it), each
//! options class reaches its service, and a callback that throws makes its builder call throw.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app_nsh::rayx_runtime::services::{
    application_configuration::ApplicationConfiguration, i18n::I18nOptions,
    input_map::InputMapOptions, navigation::NavigationOptions,
};
use rayx_app_nsh::ts::ServiceProvider;

/// The app's files: its settings and an asset catalog.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => Ok(br#"{ "RayX": { "Services": {
                "I18N": { "DefaultLanguage": "de", "ResourcesPath": "translations", "BidiIsolation": true },
                "Fonts": { "AutoRegisterAssets": true }
            }, "Application": { "Name": "From settings", "Description": "kept" } } }"#
                .to_vec()),
            "asset:/@app/assets.toml" => Ok(b"files = [\"images/logo.png\"]\n".to_vec()),
            "asset:/@app/images/logo.png" => Ok(b"png".to_vec()),
            _ => Err(format!("{uri} is not bundled")),
        }
    }
}

/// `T`'s options in `services`, as JSON.
fn options<T>(services: &Instance<ServiceProvider>) -> Result<JsonValue, String>
where
    T: Default + Clone + serde::Serialize + Send + Sync + 'static,
{
    let options = services
        .with(|services| services.services().options::<T>())
        .map_err(|error| error.to_string())?;
    serde_json::to_value(options.cloned()).map_err(|error| error.to_string())
}

/// What the test reads of the app's options from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The options the services were made with, by name.
    pub(crate) fn options(
        services: Instance<ServiceProvider>,
        name: String,
    ) -> Result<JsonValue, String> {
        match name.as_str() {
            "i18n" => options::<I18nOptions>(&services),
            "navigation" => options::<NavigationOptions>(&services),
            "inputMap" => options::<InputMapOptions>(&services),
            "application" => options::<ApplicationConfiguration>(&services),
            other => Err(format!("no options {other}")),
        }
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  ApplicationConfigurationService, I18nService, NavigationService, Probe, RayXAppBuilder,
} from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addAppSettings("app")
      .addI18n((options) => {
        options.defaultLanguage("hr").fallbackLanguage("en").autoLoadAssets(false);
        options.followPlatform(false).missingMessage("empty");
      })
      .addNavigation((options) => {
        options.addRoute("/").addRoute("/projects/{id}", { component: "Project", prewarm: true });
      })
      .addInputMap((options) => { options.bindKey("Editor", "ctrl-s", "save"); })
      .addApplicationConfiguration((options) => {
        options.name("From TS").merge({ notifications: { maxItems: 7 } });
      })
      .start();

    // Set by the callback over the file's; left alone, the file's.
    const i18n = Probe.options(services, "i18n");
    lines.push(`${i18n.default_language} ${i18n.fallback_languages} ${i18n.auto_load_assets} ${i18n.follow_platform} ${i18n.missing_message}`);
    lines.push(`${i18n.resources_path} ${i18n.bidi_isolation}`);
    lines.push(services.getRequiredService(I18nService).locale());

    const routes = Probe.options(services, "navigation").routes;
    lines.push(routes.map((route) => `${route.pattern}:${route.component}:${route.prewarm}`).join(","));
    lines.push(services.getRequiredService(NavigationService).navigate("/projects/12").status);

    const bindings = Probe.options(services, "inputMap").application;
    lines.push(bindings.map((entry) => `${entry.context}:${entry.action}`).join(","));

    const application = Probe.options(services, "application");
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
fn options_set_in_ts_apply_after_configuration_and_reach_their_services() {
    let shown = headless::run_with_assets(set(), TSX, Arc::new(AppFiles));
    let parts: Vec<&str> = shown.split('|').collect();
    assert_eq!(parts[0], "done hr en false false empty", "{shown}");
    assert_eq!(parts[1], "translations true", "{shown}");
    assert_eq!(parts[2], "hr", "{shown}");
    assert_eq!(
        parts[3], "/:null:false,/projects/{id}:Project:true",
        "{shown}"
    );
    assert_eq!(parts[4], "Success", "{shown}");
    assert_eq!(parts[5], "Editor:save", "{shown}");
    // Set by the callback over the file's; left alone, the file's.
    assert_eq!(parts[6], "From TS kept 7", "{shown}");
    assert_eq!(parts[7], "From TS", "{shown}");
    assert_eq!(parts.len(), 8, "{shown}");
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.addI18n(() => { throw new Error("no languages"); })));
  lines.push(thrown(() => builder.addNavigation(() => { throw new Error("no routes"); })));
  lines.push(thrown(() => builder.configureAppConfiguration(() => { throw new Error("no sources"); })));
  lines.push(thrown(() => builder.addI18n((options) => { options.missingMessage("loud"); })));
  lines.push(thrown(() => builder.addApplicationConfiguration((options) => {
    options.merge({ notifications: { maxItems: "many" } });
  })).startsWith("merge: ") ? "merge refused" : "merge kept");
  // An options object is valid during its callback only.
  let kept;
  builder.addNavigation((options) => { kept = options; });
  lines.push(thrown(() => kept.addRoute("/late")).includes("disposed") ? "disposed" : "kept");
  // The builder still works after a callback threw.
  try {
    await builder.addStorage().start();
    lines.push("started");
  } catch (error) {
    lines.push(`rejected ${error.message}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_its_builder_call_throw_with_its_message() {
    assert_eq!(
        headless::run(set(), THROWING),
        "done no languages|no routes|no sources\
         |missingMessage takes \"id\" or \"empty\", not \"loud\"|merge refused|disposed|started"
    );
}

#[test]
fn the_options_classes_are_declared_with_rust_s_methods() {
    let ts = headless::declarations(&set());
    for declared in [
        "export declare class I18nOptions {",
        "  defaultLanguage(language: string): I18nOptions;",
        "  fallbackLanguage(language: string): I18nOptions;",
        "  missingMessage(policy: string): I18nOptions;",
        "export declare class NavigationOptions {",
        "  addRoute(pattern: string, options?: RouteOptions | null): NavigationOptions;",
        "export declare class InputMapOptions {",
        "  bindKey(context: string, sequence: string, action: string): InputMapOptions;",
        "export declare class ConfigurationBuilder {",
        "export declare class ApplicationConfigurationOptions {",
        "  merge(settings: unknown): ApplicationConfigurationOptions;",
        "  addJson(json: string): ConfigurationBuilder;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
