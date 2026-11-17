//! Listeners of RayX's TS service classes through NSH's engine, headless: a provider gives every
//! consumer the same object of a service, so each `onChange` returns a function that stops that
//! listener alone; another consumer's keeps hearing. (The platform's and suspension's classes
//! have this group in their crates.)

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use nsh_core::prelude::*;
use rayx_app_nsh::rayx_runtime::services::i18n::I18nService;
use rayx_app_nsh::ts::ServiceProvider;

/// What the test changes and reads of the services from Rust.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Gives the i18n service messages in `en` and `hr`.
    pub(crate) fn add_languages(services: Instance<ServiceProvider>) -> Result<(), String> {
        let i18n = services
            .with(|services| services.services().required_service::<I18nService>())
            .map_err(|error| error.to_string())?;
        for language in ["en", "hr"] {
            i18n.add_resource(language, None, r#"{ "hi": "Hi" }"#)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  I18nService, NavigationService, Probe, RayXAppBuilder,
} from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder().addHostFakes()
      .addNavigation((options) => { options.addRoute("/").addRoute("/a").addRoute("/b"); })
      .addI18n((options) => { options.autoLoadAssets(false).followPlatform(false); })
      .start();

    // Two consumers, one object: each stops its own listener.
    const first = services.getRequiredService(NavigationService);
    const second = services.getRequiredService(NavigationService);
    lines.push(String(first === second));
    const routes = { first: [], second: [] };
    const stopFirst = first.onChange((event) => routes.first.push(event.to.path));
    second.onChange((event) => routes.second.push(event.to.path));
    first.navigate("/a");
    await tick();
    lines.push(String(typeof stopFirst));
    stopFirst();
    stopFirst();
    second.navigate("/b");
    await tick();
    lines.push(`routes ${routes.first} / ${routes.second}`);

    const panel = first.region("panel");
    const pages = { first: 0, second: 0 };
    const stopPanel = panel.onChange(() => pages.first++);
    panel.onChange(() => pages.second++);
    panel.navigate("/a");
    await tick();
    stopPanel();
    panel.navigate("/b");
    await tick();
    lines.push(`pages ${pages.first} / ${pages.second}`);

    Probe.addLanguages(services);
    const i18n = services.getRequiredService(I18nService);
    const locales = { first: [], second: [] };
    const stopLocale = i18n.onChange((locale) => locales.first.push(locale));
    services.getRequiredService(I18nService).onChange((locale) => locales.second.push(locale));
    i18n.setLanguage("hr");
    await tick();
    stopLocale();
    i18n.setLanguage("en");
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
fn each_consumer_stops_its_own_listener_of_a_shared_service_object() {
    let set = headless::set(|set| {
        set.module::<Probe>();
    });
    assert_eq!(
        headless::run(set, TSX),
        "done true|function|routes /a / /a,/b|pages 1 / 2|locales hr / hr,en"
    );
}

#[test]
fn listener_members_are_declared_to_return_their_stop_function() {
    let ts = headless::declarations(&headless::set(|_| {}));
    for declared in [
        "  onChange(listener: (event: unknown) => void): () => void;",
        "  onChange(listener: (locale: string) => void): () => void;",
        "  onChange(listener: (pages: unknown) => void): () => void;",
        "  onChange(listener: (settings: unknown) => void): () => void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
