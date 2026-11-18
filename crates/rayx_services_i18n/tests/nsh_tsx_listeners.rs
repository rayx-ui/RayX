//! Listeners of the `I18nService` class through NSH's engine, headless: a provider gives every
//! consumer the same object of the service, so each `onChange` returns a function that stops
//! that listener alone; another consumer's keeps hearing.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::executor::block_on;
use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx_app::services::i18n::I18nService;
use rayx_app_nsh::testing;
use rayx_app_nsh::ts::ServiceProvider;

/// What the test changes of the services from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Gives the i18n service messages in `en` and `hr`.
    pub(crate) fn add_languages(services: Instance<ServiceProvider>) -> Result<(), String> {
        let i18n = services
            .with(|services| services.services().required_service::<dyn I18nService>())
            .map_err(|error| error.to_string())?;
        for language in ["en", "hr"] {
            block_on(i18n.add_resource(language.into(), None, r#"{ "hi": "Hi" }"#.into()))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
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
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addI18n((options) => { options.autoLoadAssets(false).followPlatform(false); })
      .start();
    Probe.addLanguages(services);
    const i18n = services.getRequiredService(I18nService);
    lines.push(String(i18n === services.getRequiredService(I18nService)));
    const locales = { first: [], second: [] };
    const stopLocale = i18n.onChange((locale) => locales.first.push(locale));
    services.getRequiredService(I18nService).onChange((locale) => locales.second.push(locale));
    await i18n.setLanguage("hr");
    await tick();
    lines.push(String(typeof stopLocale));
    stopLocale();
    stopLocale();
    await i18n.setLanguage("en");
    await tick();
    lines.push(`locales ${locales.first} / ${locales.second}`);
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn each_consumer_stops_its_own_listener_of_the_i18n_service() {
    assert_eq!(
        testing::run(set(), TSX),
        "done true|function|locales hr / hr,en"
    );
}

#[test]
fn the_i18n_listener_is_declared_to_return_its_stop_function() {
    let ts = testing::declarations(&set());
    let declared = "  onChange(listener: (locale: string) => void): () => void;";
    assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
}
