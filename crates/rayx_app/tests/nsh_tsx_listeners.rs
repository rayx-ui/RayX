//! Listeners of RayX's TS service classes through NSH's engine, headless: a provider gives every
//! consumer the same object of a service, so each `onChange` (`onLifecycle`, `askBeforeQuit`)
//! returns a function that stops that listener alone; another consumer's keeps hearing, and a
//! stopped `askBeforeQuit` no longer keeps a quit request from quitting.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use nsh_core::prelude::*;
use rayx_app::rayx_runtime::services::i18n::I18nService;
use rayx_app::rayx_runtime::services::platform::PlatformInfoService;
use rayx_app::rayx_runtime::services::suspension::{
    SuspensionEvent, SuspensionEventKind, SuspensionEventReason, SuspensionService,
};
use rayx_app::ts::ServiceProvider;

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

    /// Sets the platform's viewport width to `width`.
    pub(crate) fn resize(services: Instance<ServiceProvider>, width: f64) -> Result<(), String> {
        let platform = services
            .with(|services| {
                services
                    .services()
                    .required_service::<PlatformInfoService>()
            })
            .map_err(|error| error.to_string())?;
        let mut snapshot = platform.current();
        snapshot.viewport.width = width as f32;
        platform.update(snapshot);
        Ok(())
    }

    /// Publishes a wake from sleep.
    pub(crate) fn wake(services: Instance<ServiceProvider>) -> Result<(), String> {
        suspension(&services)?.publish(SuspensionEvent::new(
            SuspensionEventKind::Woke,
            SuspensionEventReason::SystemWoke,
        ));
        Ok(())
    }

    /// Whether a quit request comes to the app instead of quitting it.
    pub(crate) fn asks_before_quit(services: Instance<ServiceProvider>) -> Result<bool, String> {
        Ok(suspension(&services)?.asks_before_quit())
    }
}

fn suspension(
    services: &Instance<ServiceProvider>,
) -> Result<std::sync::Arc<SuspensionService>, String> {
    services
        .with(|services| services.services().required_service::<SuspensionService>())
        .map_err(|error| error.to_string())
}

const TSX: &str = r#"import { mount, signal, Text } from "nsh";
import {
  I18nService, NavigationService, PlatformInfoService, Probe, RayXAppBuilder, SuspensionService,
} from "test/app";

const shown = signal("waiting");
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
setTimeout(async () => {
  const lines = [];
  try {
    const services = await new RayXAppBuilder()
      .addNavigation((options) => { options.addRoute("/").addRoute("/a").addRoute("/b"); })
      .addI18n((options) => { options.autoLoadAssets(false).followPlatform(false); })
      .addPlatform()
      .addSuspension()
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

    const platform = services.getRequiredService(PlatformInfoService);
    const widths = { first: [], second: [] };
    const stopWidth = platform.onChange((snapshot) => widths.first.push(snapshot.viewport.width));
    platform.onChange((snapshot) => widths.second.push(snapshot.viewport.width));
    Probe.resize(services, 500);
    await tick();
    stopWidth();
    Probe.resize(services, 600);
    await tick();
    lines.push(`widths ${widths.first} / ${widths.second}`);

    const suspension = services.getRequiredService(SuspensionService);
    const wakes = { first: 0, second: 0 };
    const isWake = (event) => event.kind === "Woke";
    const stopWake = suspension.onLifecycle((event) => { if (isWake(event)) wakes.first++; });
    suspension.onLifecycle((event) => { if (isWake(event)) wakes.second++; });
    Probe.wake(services);
    await tick();
    stopWake();
    Probe.wake(services);
    await tick();
    lines.push(`wakes ${wakes.first} / ${wakes.second}`);

    // A stopped `askBeforeQuit` no longer keeps a quit request from quitting.
    const asks = [Probe.asksBeforeQuit(services)];
    const stopAsking = suspension.askBeforeQuit(() => {});
    const stopAlsoAsking = services.getRequiredService(SuspensionService).askBeforeQuit(() => {});
    asks.push(Probe.asksBeforeQuit(services));
    stopAsking();
    asks.push(Probe.asksBeforeQuit(services));
    stopAlsoAsking();
    asks.push(Probe.asksBeforeQuit(services));
    lines.push(`asks ${asks}`);

    // Disposing the object stops what it still holds.
    suspension.askBeforeQuit(() => {});
    const before = Probe.asksBeforeQuit(services);
    suspension.dispose();
    lines.push(`disposed ${before} ${Probe.asksBeforeQuit(services)}`);
    lines.push(String(services.getRequiredService(SuspensionService) !== suspension));
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
        "done true|function|routes /a / /a,/b|pages 1 / 2|locales hr / hr,en\
         |widths 500 / 500,600|wakes 1 / 2|asks false,true,true,false|disposed true false|true"
    );
}

#[test]
fn listener_members_are_declared_to_return_their_stop_function() {
    let ts = headless::declarations(&headless::set(|_| {}));
    for declared in [
        "  onChange(listener: (event: unknown) => void): () => void;",
        "  onChange(listener: (locale: string) => void): () => void;",
        "  onChange(listener: (pages: unknown) => void): () => void;",
        "  onChange(listener: (snapshot: unknown) => void): () => void;",
        "  onChange(listener: (settings: unknown) => void): () => void;",
        "  onLifecycle(listener: (event: unknown) => void): () => void;",
        "  askBeforeQuit(listener: (value: unknown) => void): () => void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
