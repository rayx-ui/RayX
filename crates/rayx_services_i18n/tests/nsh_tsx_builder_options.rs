//! `addI18n` in TS through NSH's engine, headless: its options callback sets options as Rust's
//! `I18nOptions` does, after those bound from `appsettings.json` (a value it sets overrides the
//! file's, one it leaves alone keeps it), and they reach RayX's i18n service; a callback that
//! throws makes `addI18n` throw with its message; the options class and the builder methods
//! are declared with Rust's methods and doc comments.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use nsh_engine::Assets;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_services_i18n::I18nOptions;

/// The app's files: its settings.
struct AppFiles;

impl Assets for AppFiles {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        match uri {
            "asset:/@app/appsettings.json" => Ok(br#"{ "RayX": { "Services": {
                "I18N": { "DefaultLanguage": "de", "ResourcesPath": "translations", "BidiIsolation": true }
            } } }"#
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
    /// The i18n options the service was made with.
    pub(crate) fn options(services: Instance<ServiceProvider>) -> Result<JsonValue, String> {
        let options = services
            .with(|services| services.services().options::<I18nOptions>())
            .map_err(|error| error.to_string())?;
        serde_json::to_value(options.cloned()).map_err(|error| error.to_string())
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_i18n::register(set);
        set.module::<Probe>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  try {
    const builder = new RayXAppBuilder().addHostFakes().addAppSettings("app");
    const chained = builder.addI18n((options) => {
      options.defaultLanguage("hr").fallbackLanguage("en").autoLoadAssets(false);
      options.followPlatform(false).missingMessage("empty");
    });
    lines.push(String(chained === builder));
    const services = await builder.start();

    // Set by the callback over the file's; left alone, the file's.
    const i18n = Probe.options(services);
    lines.push(`${i18n.default_language} ${i18n.fallback_languages} ${i18n.auto_load_assets} ${i18n.follow_platform} ${i18n.missing_message}`);
    lines.push(`${i18n.resources_path} ${i18n.bidi_isolation}`);
    const service = services.getRequiredService(I18nService);
    lines.push(`${service instanceof I18nService} ${service.locale()} [${service.t("missing")}]`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn options_set_in_ts_apply_after_configuration_and_reach_the_i18n_service() {
    let shown = testing::run_with_assets(set(), TSX, Arc::new(AppFiles));
    assert_eq!(
        shown,
        "done true|hr en false false empty|translations true|true hr []"
    );
}

const THROWING: &str = r#"import { mount, signal, Text } from "nsh";
import { RayXAppBuilder } from "test/app";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
  const builder = new RayXAppBuilder().addHostFakes();
  lines.push(thrown(() => builder.addI18n(() => { throw new Error("no languages"); })));
  lines.push(thrown(() => builder.addI18n((options) => { options.missingMessage("loud"); })));
  // The builder still works after a callback threw.
  try {
    await builder.addI18n().start();
    lines.push("started");
  } catch (error) {
    lines.push(`rejected ${error.message}`);
  }
  lines.push(thrown(() => builder.addI18n()).includes("started") ? "started already" : "kept");
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_throwing_callback_makes_add_i18n_throw_with_its_message() {
    assert_eq!(
        testing::run(set(), THROWING),
        "done no languages|missingMessage takes \"id\" or \"empty\", not \"loud\"|started|started already"
    );
}

#[test]
fn the_i18n_options_class_and_builder_methods_are_declared_with_rust_s_methods() {
    let set = set();
    let ts = testing::declarations(&set);
    for declared in [
        "export declare class I18nOptions {",
        "  defaultLanguage(language: string): I18nOptions;",
        "  fallbackLanguage(language: string): I18nOptions;",
        "  missingMessage(policy: string): I18nOptions;",
        "export declare class I18nService {",
        "  get contract(): I18nServiceContract | null;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    // `addI18n` and `addMessageFormatter` carry their doc comments.
    for (method, about) in [
        (
            "addI18n(configure?: ((options: I18nOptions) => void) | null): RayXAppBuilder;",
            "i18n service",
        ),
        ("addMessageFormatter(): RayXAppBuilder;", "formatter"),
    ] {
        let at = ts
            .find(method)
            .unwrap_or_else(|| panic!("no `{method}` in\n{ts}"));
        let before = ts[..at].trim_end();
        assert!(
            before.ends_with("*/"),
            "no doc comment on {method} in\n{ts}"
        );
        let doc = &before[before.rfind("/**").unwrap()..];
        assert!(doc.contains(about), "{doc}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}
