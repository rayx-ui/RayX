//! The `I18nService` class's commands through NSH's engine, headless: `setLanguage`,
//! `followPlatform`, `setTimeZone`, `loadMessages` and `addResource` return promises that
//! resolve once the service took the command and reject with its error, over RayX's service and
//! over a TS implementation of the contract alike.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod messages;

use nsh_core::element::Registry;
use nsh_core::prelude::*;
use rayx_app_nsh::RayXAppBuilder;
use rayx_app_nsh::testing;
use rayx_services_i18n::I18nAppBuilderExt as _;

/// The app's Rust side: its composition root, for the TSX builder.
#[module]
struct Probe;

#[module]
impl Probe {
    /// Adds RayX's i18n service to `builder`, over messages in English and a German file that
    /// does not read, loaded when the app asks.
    pub(crate) fn add_app(
        builder: Instance<rayx_app_nsh::ts::RayXAppBuilder>,
    ) -> Result<Instance<rayx_app_nsh::ts::RayXAppBuilder>, String> {
        builder
            .with_mut(|builder| {
                builder.extend(|builder: RayXAppBuilder| {
                    messages::add_messages(
                        builder,
                        &[
                            ("locales/en.json", r#"{ "hello": "Hello" }"#),
                            ("locales/de.json", "{ not json"),
                        ],
                    )
                    .add_i18n(|options| {
                        options.auto_load_assets(false).follow_platform(false);
                    })
                })
            })
            .map_err(|error| error.to_string())?;
        Ok(builder)
    }
}

fn set() -> Registry {
    testing::set(|set| {
        rayx_services_i18n::register(set);
        set.module::<Probe>();
    })
}

const RAYX: &str = r#"import { mount, signal, Text } from "nsh";
import { I18nService, Probe, RayXAppBuilder } from "test/app";

const shown = signal("waiting");
const rejection = (promise) => promise.then(() => "resolved", (error) => `rejected ${error}`);
setTimeout(async () => {
  const lines = [];
  try {
    const services = await Probe.addApp(new RayXAppBuilder()).addHostFakes().start();
    const i18n = services.getRequiredService(I18nService);
    lines.push(i18n.has("hello") ? "loaded" : "not loaded");
    // The messages that load are added; the promise rejects naming the one that did not.
    const loaded = await rejection(i18n.loadMessages());
    lines.push(loaded.includes("de") ? "load rejects naming de" : loaded);
    lines.push(i18n.t("hello"));
    await i18n.addResource("hr", "shop", JSON.stringify({ buy: "Kupi" }));
    await i18n.addResource("hr", null, JSON.stringify({ ok: "U redu" }));
    const bad = await rejection(i18n.addResource("hr", null, JSON.stringify({ bad: "{$x" })));
    lines.push(bad.startsWith("rejected") ? "bad resource rejects" : bad);
    // Each command has taken effect once its promise resolves.
    await i18n.setLanguage("hr");
    lines.push(`${i18n.locale()} ${i18n.t("shop.buy")} ${i18n.t("ok")}`);
    const tag = await rejection(i18n.setLanguage("not a tag"));
    lines.push(tag.includes("not a tag") ? "bad tag rejects" : tag);
    await i18n.setTimeZone("Asia/Tokyo");
    lines.push(i18n.timeZone());
    const zone = await rejection(i18n.setTimeZone("Europe/Atlantis"));
    lines.push(zone.startsWith("rejected") ? "bad zone rejects" : zone);
    // The host fakes' platform: American English on a 12-hour clock.
    await i18n.followPlatform();
    lines.push(i18n.locale());
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_class_s_commands_resolve_once_rayx_s_service_took_them_and_reject_with_its_errors() {
    assert_eq!(
        testing::run(set(), RAYX),
        "done not loaded|load rejects naming de|Hello|bad resource rejects|hr Kupi U redu|bad tag rejects|Asia/Tokyo|bad zone rejects|en-US-u-hc-h12"
    );
}

const TS_IMPLEMENTATION: &str = r#"import { mount, signal, Text } from "nsh";
import { ContractState, I18nService, RayXAppBuilder } from "test/app";

const tick = () => new Promise((resolve) => setTimeout(resolve, 10));

/// An i18n service written in TS that takes its time, and refuses what it does not know. It
/// publishes without counting versions.
class SlowI18n {
  calls = [];
  i18n = new ContractState({
    language: "en", locale: "en", default_language: "en", time_zone: "UTC",
    available_languages: ["en", "hr"],
    messages: [
      { language: "en", messages: { hello: "Hello" } },
      { language: "hr", messages: { hello: "Bok" } },
    ],
  });
  async setLanguage(language) {
    await tick();
    if (language !== "en" && language !== "hr") throw new Error(`no ${language} here`);
    this.i18n.publish({ ...this.i18n.value, language, locale: language, chosen_language: language });
  }
  async followPlatform() {
    await tick();
    this.i18n.publish({ ...this.i18n.value, language: "en", locale: "en", chosen_language: null });
  }
  async setTimeZone(zone) {
    await tick();
    this.i18n.publish({ ...this.i18n.value, time_zone: zone });
  }
  async loadMessages() {
    await tick();
    throw new Error("the server is away");
  }
  async addResource(language, namespace, json) {
    await tick();
    this.calls.push(`${language} ${namespace} ${json}`);
  }
}

const shown = signal("waiting");
const rejection = (promise) => promise.then(() => "resolved", (error) => `rejected ${error}`);
setTimeout(async () => {
  const lines = [];
  try {
    const implementation = new SlowI18n();
    const services = await new RayXAppBuilder().addHostFakes()
      .addSingletonAs(I18nService, () => implementation)
      .addMessageFormatter()
      .start();
    const i18n = services.getRequiredService(I18nService);
    await i18n.setLanguage("hr");
    lines.push(`${i18n.locale()} ${i18n.t("hello")}`);
    const refused = await rejection(i18n.setLanguage("fr"));
    lines.push(refused.includes("no fr here") ? "refusal rejects" : refused);
    await i18n.setTimeZone("Asia/Tokyo");
    lines.push(i18n.timeZone());
    await i18n.followPlatform();
    lines.push(`${i18n.locale()} ${i18n.t("hello")}`);
    const load = await rejection(i18n.loadMessages());
    lines.push(load.includes("the server is away") ? "load rejects" : load);
    await i18n.addResource("hr", null, "{}");
    lines.push(implementation.calls.join(","));
  } catch (error) {
    lines.push(`threw ${error}`);
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn the_class_s_commands_wait_for_a_ts_implementation_and_reject_with_its_errors() {
    assert_eq!(
        testing::run(set(), TS_IMPLEMENTATION),
        "done hr Bok|refusal rejects|Asia/Tokyo|en Hello|load rejects|hr null {}"
    );
}

#[test]
fn the_class_s_commands_are_declared_as_promises() {
    let ts = testing::declarations(&set());
    for declared in [
        "  setLanguage(language: string): Promise<void>;",
        "  followPlatform(): Promise<void>;",
        "  setTimeZone(timeZone: string): Promise<void>;",
        "  loadMessages(): Promise<void>;",
        "  addResource(language: string, namespace: string | null, json: string): Promise<void>;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
