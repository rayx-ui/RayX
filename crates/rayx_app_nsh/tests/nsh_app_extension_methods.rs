//! `#[app_extension]` as a TS builder method, headless: `fn add_weather(builder) -> builder`,
//! registered by the app's set with `registry.class_extension::<add_weather>()`, is
//! `builder.addWeather()` on every TS `RayXAppBuilder`, in JS and in the declarations with the
//! function's docs; it chains with RayX's own methods and throws once the builder started.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use nsh_core::prelude::*;
use rayx_app_nsh::ts::ServiceProvider;
use rayx_app_nsh::{AppService, RayXAppBuilder, ServiceAppBuilderExt as _};

/// A forecast the extension registers.
#[derive(AppService)]
struct Forecast(&'static str);

/// Registers a sunny forecast.
#[rayx_app_nsh::app_extension]
fn add_weather(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_singleton(Forecast("sunny"))
}

/// The test's Rust side.
#[module]
struct Probe;

#[module]
impl Probe {
    /// The forecast the app registered.
    pub(crate) fn forecast(services: Instance<ServiceProvider>) -> String {
        services
            .with(|services| services.services().required_service::<Forecast>())
            .map(|forecast| forecast.0.to_owned())
            .unwrap_or_else(|error| error.to_string())
    }
}

fn set() -> nsh_core::element::Registry {
    headless::set(|set| {
        set.module::<Probe>().class_extension::<add_weather>();
    })
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
const thrown = (f) => { try { f(); return "no throw"; } catch (e) { return e.message; } };
setTimeout(async () => {
  const lines = [];
  try {
    lines.push(typeof RayXAppBuilder.prototype.addWeather);
    // Chained with RayX's own methods: the method returns the builder itself.
    const builder = new RayXAppBuilder().addHostFakes().addStorage();
    lines.push(String(builder.addWeather() === builder));
    const services = await builder.addI18n().start();
    lines.push(Probe.forecast(services));
    lines.push(String(services.getRequiredService(I18nService) instanceof I18nService));
    // After `start()`, it throws.
    lines.push(thrown(() => builder.addWeather()).includes("started") ? "started" : "open");
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn an_app_extension_is_a_builder_method() {
    assert_eq!(
        headless::run(set(), TSX),
        "done function|true|sunny|true|started"
    );
}

#[test]
fn the_method_is_declared_on_the_builder_with_the_function_s_docs() {
    let set = set();
    let ts = headless::declarations(&set);
    assert!(
        ts.contains("  /** Registers a sunny forecast. */\n  addWeather(): RayXAppBuilder;"),
        "{ts}"
    );
    // No flat function any more.
    assert!(!ts.contains("export declare function addWeather"), "{ts}");
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
fn without_the_app_s_set_the_builder_has_no_such_method() {
    let ts = headless::declarations(&headless::set(|set| {
        set.module::<Probe>();
    }));
    assert!(!ts.contains("addWeather(): RayXAppBuilder"), "{ts}");
}
